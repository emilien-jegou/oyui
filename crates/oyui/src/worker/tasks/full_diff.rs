//! Full file diff task: async reads plus spawn_blocking hunk computation.
use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::{Listener, TaskerContext};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

use crate::cli::DiffAlgorithm;
use crate::diff::{line_ranges, DiffLine, DiffResult, FileDiff, Hunk, InlineChange};
use crate::diff_cache::{DiffCache, LineIndex, LineWidths};
use crate::worker::events::diff_update::DiffUpdate;

const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB limit

pub struct FullDiff;

#[derive(Clone)]
pub struct FullDiffReq {
    pub node_path: PathBuf,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    /// Cache generation this task's claim was made under.
    pub generation: u64,
}

#[derive(TaskerContext)]
pub struct FullDiffContext {
    pub cache: DiffCache,
    pub algorithm: DiffAlgorithm,
}

impl Listener<FullDiffReq> for FullDiff {
    type Sender = crate::worker::EventSender;
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
                load_file_content_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let right_fut = async {
            if let Some(p) = &right_path_clone {
                load_file_content_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let (left_res, right_res) = tokio::join!(left_fut, right_fut);

        let diff_result = match (left_res, right_res) {
            (Err(e), _) | (_, Err(e)) => e,
            (Ok(left_text), Ok(right_text)) => {
                if left_text.is_empty() && right_text.is_empty() {
                    ctx.cache.diffs.set(
                        event.node_path.clone(),
                        Arc::new(DiffResult::Empty),
                        event.generation,
                    );
                    let _ = tx.send(DiffUpdate {
                        path: event.node_path,
                        diff_result: DiffResult::Empty,
                    });
                    return Ok(());
                }

                let algorithm = ctx.algorithm;
                let node_path = event.node_path.clone();
                tokio::task::spawn_blocking(move || {
                    match compute(&algorithm, &left_text, &right_text, &node_path) {
                        Ok(hunks) => DiffResult::Text(FileDiff {
                            old_file_content: Arc::from(left_text),
                            new_file_content: Arc::from(right_text),
                            hunks,
                            line_selections: Default::default(),
                        }),
                        Err(e) => DiffResult::Error(e.to_string()),
                    }
                })
                .await
                .unwrap_or_else(|join_err| {
                    tracing::error!(?join_err, "diff computation task panicked");
                    DiffResult::Error(format!("diff task panicked: {join_err}"))
                })
            }
        };

        tracing::trace!("Full diff computation finished");

        // Derived layout data the view would otherwise recompute per frame:
        // horizontal scrolling needs the widest line, and rendering needs the
        // line offsets. Both are read off the content we already hold here.
        if let DiffResult::Text(ref diff) = diff_result {
            let generation = ctx.cache.line_index.generation();
            let node_path = event.node_path.clone();
            let widths = Arc::new(LineWidths {
                old: widest_line(&diff.old_file_content),
                new: widest_line(&diff.new_file_content),
            });
            let index = Arc::new(LineIndex::of(diff));
            ctx.cache
                .line_widths
                .set(node_path.clone(), widths, generation);
            ctx.cache.line_index.set(node_path, index, generation);
        }

        ctx.cache.diffs.set(
            event.node_path.clone(),
            Arc::new(diff_result.clone()),
            event.generation,
        );
        let _ = tx.send(DiffUpdate {
            path: event.node_path,
            diff_result,
        });
        Ok(())
    }
}

/// Widest line of `content` in display columns (tabs stay one column here, as
/// the gutter expands them).
fn widest_line(content: &str) -> usize {
    content
        .lines()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
}

async fn load_file_content_safely(path: &PathBuf) -> Result<String, DiffResult> {
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

/// Byte range of line `line_idx`, with the terminator stripped.
///
/// Mirrors the ranges published for the renderer, so the inline-highlight
/// offsets the diff computes and the ones the view slices with agree.
fn printable_byte_range(text: &str, ranges: &[Range<usize>], line_idx: usize) -> Range<usize> {
    ranges
        .get(line_idx)
        .cloned()
        .unwrap_or_else(|| text.len()..text.len())
}

pub fn compute(
    algo: &DiffAlgorithm,
    left_file_content: &str,
    right_file_content: &str,
    path: &Path,
) -> Result<Vec<Hunk>, Box<dyn std::error::Error + Send + Sync>> {
    let input = InternedInput::new(left_file_content, right_file_content);

    let inner_algo = match algo {
        DiffAlgorithm::Histogram | DiffAlgorithm::SyntaxAware => Algorithm::Histogram,
        DiffAlgorithm::Myers => Algorithm::Myers,
        DiffAlgorithm::MyersMinimal => Algorithm::MyersMinimal,
    };

    let diff = Diff::compute(inner_algo, &input);

    let old_ranges = line_ranges(left_file_content);
    let new_ranges = line_ranges(right_file_content);

    let syntax_res = if *algo == DiffAlgorithm::SyntaxAware {
        match oyui_syndiff::diff_source(left_file_content, right_file_content, path, None) {
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
            let line_range = printable_byte_range(left_file_content, &old_ranges, i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.old_ranges.as_slice())
                    .unwrap_or(&[]),
                left_file_content,
            );
            lines.push(DiffLine::Deletion {
                old_line_idx: i,
                inline_highlights,
            });
        }

        for i in (hunk.after.start as usize)..(hunk.after.end as usize) {
            let line_range = printable_byte_range(right_file_content, &new_ranges, i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.new_ranges.as_slice())
                    .unwrap_or(&[]),
                right_file_content,
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
            marker: Default::default(),
        });
    }

    Ok(hunks)
}
