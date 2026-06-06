use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

use crate::cli::DiffAlgorithm;
use crate::commons::lazy::Lazy;
use crate::diff::{DiffLine, DiffResult, FileDiff, Hunk, InlineChange};
use crate::diff_cache::DiffCache;

const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB limit

pub struct FullDiff;

#[derive(Debug, Clone)]
pub struct FullDiffReq {
    pub node_path: PathBuf,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FullDiffRes {
    pub node_path: PathBuf,
    pub result: DiffResult,
    pub right_path: Option<PathBuf>,
}

async fn load_text_safely(path: &PathBuf) -> Result<String, DiffResult> {
    let meta = match fs::metadata(path).await {
        Ok(m) => m,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let size = meta.len();

    if size > MAX_FILE_SIZE {
        return Err(DiffResult::TooLarge(size));
    }

    let buffer = match fs::read(path).await {
        Ok(b) => b,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let get_binary_info = |buf: &[u8]| -> DiffResult {
        let kind = infer::get(buf);
        let mime = kind
            .map(|k| k.mime_type().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let ext = kind
            .map(|k| k.extension().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        DiffResult::Binary { size, mime, ext }
    };

    let check_len = std::cmp::min(buffer.len(), 8000);
    if buffer[..check_len].contains(&0) {
        return Err(get_binary_info(&buffer));
    }

    match String::from_utf8(buffer) {
        Ok(text) => Ok(text),
        Err(e) => {
            let original_buffer = e.into_bytes();
            Err(get_binary_info(&original_buffer))
        }
    }
}

struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { text, starts }
    }

    fn printable_byte_range(&self, line_idx: usize) -> Range<usize> {
        let start = self.starts.get(line_idx).copied().unwrap_or(0);
        let mut end = self
            .starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.text.len());

        let bytes = self.text.as_bytes();
        while end > start {
            let b = bytes[end - 1];
            if b == b'\n' || b == b'\r' {
                end -= 1;
            } else {
                break;
            }
        }
        start..end
    }
}

pub fn compute(
    algo: &DiffAlgorithm,
    left_text: &str,
    right_text: &str,
    path: &Path,
) -> Result<Vec<Hunk>, Box<dyn std::error::Error + Send + Sync>> {
    let input = InternedInput::new(left_text, right_text);

    let inner_algo = match algo {
        DiffAlgorithm::Histogram | DiffAlgorithm::SyntaxAware => Algorithm::Histogram,
        DiffAlgorithm::Myers => Algorithm::Myers,
        DiffAlgorithm::MyersMinimal => Algorithm::MyersMinimal,
    };

    let diff = Diff::compute(inner_algo, &input);

    let old_idx = LineIndex::new(left_text);
    let new_idx = LineIndex::new(right_text);

    let syntax_res = if *algo == DiffAlgorithm::SyntaxAware {
        match oyui_syndiff::diff_source(left_text, right_text, path, None) {
            Ok(res) => Some(res),
            Err(e) => {
                tracing::debug!(
                    "Syntax diff unavailable/failed ({}). Falling back to text.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let mut hunks = Vec::new();

    for hunk in diff.hunks() {
        let mut lines = Vec::new();

        let get_highlights = |line_range: Range<usize>,
                              struct_ranges: &[Range<usize>],
                              text: &str|
         -> Vec<InlineChange> {
            let line_text = &text[line_range.start..line_range.end];
            let trimmed_len = line_text.trim().len();

            if trimmed_len == 0 {
                return Vec::new(); 
            }

            if let Some(_syntax) = &syntax_res {
                let mut raw_ranges = Vec::new();

                for r in struct_ranges {
                    if r.start < line_range.end && r.end > line_range.start {
                        let clamp_start = r.start.max(line_range.start) - line_range.start;
                        let clamp_end = r.end.min(line_range.end) - line_range.start;
                        raw_ranges.push(clamp_start..clamp_end);
                    }
                }

                if raw_ranges.is_empty() {
                    return Vec::new();
                }

                raw_ranges.sort_by_key(|r| r.start);
                let mut merged = vec![raw_ranges[0].clone()];
                for r in raw_ranges.into_iter().skip(1) {
                    let last = merged.last_mut().unwrap();
                    if r.start <= last.end {
                        last.end = last.end.max(r.end);
                    } else {
                        merged.push(r);
                    }
                }

                let total_highlighted: usize = merged.iter().map(|r| r.end - r.start).sum();

                if total_highlighted >= trimmed_len {
                    return Vec::new();
                }

                return merged
                    .into_iter()
                    .map(|byte_range| InlineChange { byte_range })
                    .collect();
            }

            Vec::new()
        };

        for i in (hunk.before.start as usize)..(hunk.before.end as usize) {
            let line_range = old_idx.printable_byte_range(i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.old_ranges.as_slice())
                    .unwrap_or(&[]),
                left_text,
            );
            lines.push(DiffLine::Deletion {
                old_line_idx: i,
                inline_highlights,
            });
        }

        for i in (hunk.after.start as usize)..(hunk.after.end as usize) {
            let line_range = new_idx.printable_byte_range(i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.new_ranges.as_slice())
                    .unwrap_or(&[]),
                right_text,
            );
            lines.push(DiffLine::Addition {
                new_line_idx: i,
                inline_highlights,
            });
        }

        hunks.push(Hunk {
            before_lines: (hunk.before.start as usize)..(hunk.before.end as usize),
            after_lines: (hunk.after.start as usize)..(hunk.after.end as usize),
            lines,
            marker: Default::default()
        });
    }

    Ok(hunks)
}

#[derive(TaskerContext)]
pub struct FullDiffContext {
    pub algorithm: DiffAlgorithm,
}

impl Listener<FullDiffReq, crate::worker::EventSender> for FullDiff {
    type Context = FullDiffContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(
        event: FullDiffReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!(
            left_path = ?event.left_path,
            right_path = ?event.right_path,
            "Computing full diff"
        );

        let left_path_clone = event.left_path.clone();
        let right_path_clone = event.right_path.clone();

        let left_fut = async {
            if let Some(p) = &left_path_clone {
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let right_fut = async {
            if let Some(p) = &right_path_clone {
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let (left_res, right_res) = tokio::join!(left_fut, right_fut);

        let diff_result = match (left_res, right_res) {
            (Err(e), _) | (_, Err(e)) => e,
            (Ok(left_text), Ok(right_text)) => {
                if left_text.is_empty() && right_text.is_empty() {
                    tx.send(FullDiffRes {
                        node_path: event.node_path.clone(),
                        result: DiffResult::Empty,
                        right_path: event.right_path.clone(),
                    })?;
                    return Ok(());
                }

                match compute(&ctx.algorithm, &left_text, &right_text, &event.node_path) {
                    Ok(hunks) => DiffResult::Text(FileDiff {
                        old_text: Arc::from(left_text),
                        new_text: Arc::from(right_text),
                        hunks,
                        line_selections: Default::default(),
                    }),
                    Err(e) => DiffResult::Error(e.to_string()),
                }
            }
        };

        tracing::trace!("Full diff computation finished");
        tx.send(FullDiffRes {
            node_path: event.node_path.clone(),
            result: diff_result,
            right_path: event.right_path.clone(),
        })?;
        Ok(())
    }
}

#[derive(TaskerContext)]
pub struct FullDiffResCtx {
    pub cache: Arc<RwLock<DiffCache>>,
    pub syntax_theme: Arc<RwLock<Lazy<Arc<syntect::highlighting::Theme>>>>,
}

pub struct FullDiffResListener;
impl Listener<FullDiffRes, crate::worker::EventSender> for FullDiffResListener {
    type Context = FullDiffResCtx;

    async fn handle(
        event: FullDiffRes,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!(node_path = %event.node_path.display(), "Applied FullDiff cache");

        let mut cache = ctx.cache.write();
        if let crate::diff::DiffResult::Text(ref file_diff) = event.result {
            let text = file_diff.new_text.clone();
            cache.syntax.mark_started(event.node_path.clone());

            tracing::trace!(node_path = %event.node_path.display(), "Queueing Syntax task");
            if let Some(val) = ctx.syntax_theme.read().value() {
                let _ = tx.send(crate::worker::tasks::syntax::SyntaxReq {
                    node_path: event.node_path.clone(),
                    text,
                    right_path: event.right_path.clone(),
                    theme: val.clone(),
                });
            }
        }

        cache.diffs.set(event.node_path, event.result);
        Ok(())
    }
}
