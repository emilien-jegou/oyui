//! Full file diff task: async reads plus spawn_blocking hunk computation.
use crate::cli::DiffAlgorithm;
use crate::diff::{line_ranges, DiffLine, DiffResult, FileDiff, Hunk, InlineChange};
use crate::diff_cache::{DiffCache, LineIndex, LineWidths};
use crate::worker::events::diff_update::DiffUpdate;
use imara_diff::{Diff, InternedInput};
use oyui_syndiff::LineOp;
use oyui_tasker::{Listener, TaskerContext};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
                load_file_content_safely(p)
            } else {
                Ok(String::new())
            }
        };
        let right_fut = async {
            if let Some(p) = &right_path_clone {
                load_file_content_safely(p)
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
                        path: event.node_path.clone(),
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
                            revision: FileDiff::next_revision(),
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
            Arc::new(diff_result),
            event.generation,
        );
        let _ = tx.send(DiffUpdate {
            path: event.node_path,
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

/// Shared file loader for both the diff pass and the stats pass.
///
/// Returns the decoded text, or an [`DiffResult`] describing why no text
/// could be produced, so every consumer classifies a file identically.
pub(crate) fn load_file_content_safely(path: &Path) -> Result<String, DiffResult> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let size = meta.len();

    if size > MAX_FILE_SIZE {
        return Err(DiffResult::TooLarge(size));
    }

    let buffer = match std::fs::read(path) {
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

pub fn compute(
    algo: &DiffAlgorithm,
    left_file_content: &str,
    right_file_content: &str,
    path: &Path,
) -> Result<Vec<Hunk>, Box<dyn std::error::Error + Send + Sync>> {
    let input = InternedInput::new(left_file_content, right_file_content);

    let diff = Diff::compute(algo.as_imara(), &input);

    let old_ranges = line_ranges(left_file_content);
    let new_ranges = line_ranges(right_file_content);

    // Structural mode: the tree diff drives the line alignment.
    if *algo == DiffAlgorithm::SyntaxAware {
        match structural_hunks(left_file_content, right_file_content, path) {
            Ok(hunks) => return Ok(hunks),
            Err(e) => {
                tracing::debug!(
                    "Structural line diff unavailable ({}); falling back to plain line diff",
                    e
                );
            }
        }
    }

    // Line modes: parse once for token-level inline highlights; an
    // unsupported/broken file simply yields plain deletion/addition rows.
    let trees =
        match oyui_syndiff::parse_into_trees(left_file_content, right_file_content, path, None) {
            Ok(trees) => Some(trees),
            Err(e) => {
                tracing::debug!("Inline highlights unavailable ({}); plain line diff", e);
                None
            }
        };

    let mut hunks = Vec::new();

    for hunk in diff.hunks() {
        let before = (hunk.before.start as usize)..(hunk.before.end as usize);
        let after = (hunk.after.start as usize)..(hunk.after.end as usize);

        let (del_highlights, add_highlights) = token_highlights(
            trees.as_ref(),
            left_file_content,
            right_file_content,
            &before,
            &after,
            &old_ranges,
            &new_ranges,
        );

        let mut lines = Vec::with_capacity(before.len() + after.len());

        for (k, i) in before.clone().enumerate() {
            lines.push(DiffLine::Deletion {
                old_line_idx: i,
                inline_highlights: del_highlights[k].clone(),
            });
        }

        for (k, i) in after.clone().enumerate() {
            lines.push(DiffLine::Addition {
                new_line_idx: i,
                inline_highlights: add_highlights[k].clone(),
            });
        }

        hunks.push(Hunk {
            before_lines: before,
            after_lines: after,
            lines,
            marker: Default::default(),
        });
    }

    Ok(hunks)
}

/// Byte span of a contiguous line window, or `None` when the window is empty.
fn line_block_span(ranges: &[Range<usize>], lines: &Range<usize>) -> Option<Range<usize>> {
    if lines.is_empty() {
        return None;
    }
    let start = ranges.get(lines.start)?.start;
    let end = ranges.get(lines.end - 1)?.end;
    (start < end).then_some(start..end)
}

/// Token-level inline highlights for one imara hunk: diffs the changed byte
/// blocks on both sides and buckets the novel token ranges per line.
fn token_highlights(
    trees: Option<&(oyui_syndiff::SyntaxTree, oyui_syndiff::SyntaxTree)>,
    left: &str,
    right: &str,
    before: &Range<usize>,
    after: &Range<usize>,
    old_ranges: &[Range<usize>],
    new_ranges: &[Range<usize>],
) -> (Vec<Vec<InlineChange>>, Vec<Vec<InlineChange>>) {
    let mut del = vec![Vec::new(); before.len()];
    let mut add = vec![Vec::new(); after.len()];

    let Some((lhs_tree, rhs_tree)) = trees else {
        return (del, add);
    };

    let (Some(lhs_span), Some(rhs_span)) = (
        line_block_span(old_ranges, before),
        line_block_span(new_ranges, after),
    ) else {
        return (del, add);
    };

    let res = oyui_syndiff::diff_inline(lhs_tree, left, lhs_span, rhs_tree, right, rhs_span);

    bucket_to_lines(&res.lhs_ranges, old_ranges, before, &mut del);
    bucket_to_lines(&res.rhs_ranges, new_ranges, after, &mut add);

    (del, add)
}

/// Assigns absolute byte ranges to the lines of a window, producing
/// line-relative [`InlineChange`]s; adjacent ranges merge.
fn bucket_to_lines(
    ranges: &[Range<usize>],
    line_ranges: &[Range<usize>],
    lines: &Range<usize>,
    out: &mut [Vec<InlineChange>],
) {
    for range in ranges {
        for (k, line_idx) in lines.clone().enumerate() {
            let Some(line) = line_ranges.get(line_idx) else {
                break;
            };
            let start = range.start.max(line.start);
            let end = range.end.min(line.end);
            if start < end {
                out[k].push(InlineChange {
                    byte_range: (start - line.start)..(end - line.start),
                });
            }
        }
    }

    for line_highlights in out.iter_mut() {
        line_highlights.sort_by_key(|c| c.byte_range.start);
        let mut merged: Vec<InlineChange> = Vec::new();
        for change in line_highlights.drain(..) {
            match merged.last_mut() {
                Some(last) if change.byte_range.start <= last.byte_range.end => {
                    last.byte_range.end = last.byte_range.end.max(change.byte_range.end);
                }
                _ => merged.push(change),
            }
        }
        *line_highlights = merged;
    }
}

/// Structural mode (difftastic-style): groups the structural line ops into
/// hunks. `Equal` rows are context and are not part of any hunk; `Replaced`
/// rows become a matched deletion/addition pair, `Deleted`/`Added` rows
/// plain deletions/additions.
fn structural_hunks(
    left: &str,
    right: &str,
    path: &Path,
) -> Result<Vec<Hunk>, oyui_syndiff::FailedDiff> {
    let (lhs_tree, rhs_tree) = oyui_syndiff::parse_into_trees(left, right, path, None)?;
    let line_diff = oyui_syndiff::diff_line_ops(&lhs_tree, left, &rhs_tree, right, None)
        .ok_or(oyui_syndiff::FailedDiff::GraphLimitExceeded)?;

    let mut hunks: Vec<Hunk> = Vec::new();

    let flush_run = |run: &mut Vec<&LineOp>, hunks: &mut Vec<Hunk>| {
        if run.is_empty() {
            return;
        }

        let before_min = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Deleted { lhs } | LineOp::Replaced { lhs, .. } => Some(*lhs),
                _ => None,
            })
            .min();
        let before_max = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Deleted { lhs } | LineOp::Replaced { lhs, .. } => Some(*lhs),
                _ => None,
            })
            .max();

        let after_min = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Added { rhs } | LineOp::Replaced { rhs, .. } => Some(*rhs),
                _ => None,
            })
            .min();
        let after_max = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Added { rhs } | LineOp::Replaced { rhs, .. } => Some(*rhs),
                _ => None,
            })
            .max();

        let mut lines = Vec::new();

        let mut deletions: Vec<(usize, Vec<Range<usize>>)> = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Deleted { lhs } => Some((*lhs, Vec::new())),
                LineOp::Replaced {
                    lhs, lhs_inline, ..
                } => Some((*lhs, lhs_inline.clone())),
                _ => None,
            })
            .collect();
        deletions.sort_by_key(|(idx, _)| *idx);

        let mut additions: Vec<(usize, Vec<Range<usize>>)> = run
            .iter()
            .filter_map(|op| match op {
                LineOp::Added { rhs } => Some((*rhs, Vec::new())),
                LineOp::Replaced {
                    rhs, rhs_inline, ..
                } => Some((*rhs, rhs_inline.clone())),
                _ => None,
            })
            .collect();
        additions.sort_by_key(|(idx, _)| *idx);

        for (idx, inline) in deletions {
            lines.push(DiffLine::Deletion {
                old_line_idx: idx,
                inline_highlights: inline
                    .into_iter()
                    .map(|byte_range| InlineChange { byte_range })
                    .collect(),
            });
        }

        for (idx, inline) in additions {
            lines.push(DiffLine::Addition {
                new_line_idx: idx,
                inline_highlights: inline
                    .into_iter()
                    .map(|byte_range| InlineChange { byte_range })
                    .collect(),
            });
        }

        hunks.push(Hunk {
            before_lines: before_min.unwrap()..before_max.unwrap() + 1,
            after_lines: after_min.unwrap()..after_max.unwrap() + 1,
            lines,
            marker: Default::default(),
        });

        run.clear();
    };

    let mut run: Vec<&LineOp> = Vec::new();

    for op in &line_diff.ops {
        if matches!(op, LineOp::Equal { .. }) {
            flush_run(&mut run, &mut hunks);
        } else {
            run.push(op);
        }
    }
    flush_run(&mut run, &mut hunks);

    Ok(hunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collects the highlighted slices of every hunk row of one side.
    fn row_texts(
        hunks: &[Hunk],
        content: &str,
        ranges: &[Range<usize>],
        want_del: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();

        for hunk in hunks {
            for line in &hunk.lines {
                let (idx, highlights) = match line {
                    DiffLine::Deletion {
                        old_line_idx,
                        inline_highlights,
                    } if want_del => (*old_line_idx, inline_highlights),
                    DiffLine::Addition {
                        new_line_idx,
                        inline_highlights,
                    } if !want_del => (*new_line_idx, inline_highlights),
                    _ => continue,
                };
                let line_span = ranges[idx].clone();
                for change in highlights {
                    out.push(
                        content[line_span.start + change.byte_range.start
                            ..line_span.start + change.byte_range.end]
                            .to_string(),
                    );
                }
            }
        }

        out
    }

    #[test]
    fn mode_a_highlights_novel_tokens() {
        let left = "fn a() {\n    work(foo);\n}\n";
        let right = "fn a() {\n    work(bar);\n}\n";

        let hunks =
            compute(&DiffAlgorithm::Myers, left, right, Path::new("x.rs")).expect("line diff");

        // line ranges of both sides
        let old = line_ranges(left);
        let new = line_ranges(right);

        let del = row_texts(&hunks, left, &old, true);
        let add = row_texts(&hunks, right, &new, false);

        assert_eq!(del, vec!["foo"]);
        assert_eq!(add, vec!["bar"]);
    }

    #[test]
    fn mode_a_pure_additions_highlight_nothing() {
        let left = "fn a() {}\n";
        let right = "fn a() {}\nfn extra() {}\n";

        let hunks =
            compute(&DiffAlgorithm::Myers, left, right, Path::new("x.rs")).expect("line diff");

        assert!(!hunks.is_empty());
        // Whole-line novel rows carry no inline ranges.
        for hunk in &hunks {
            for line in &hunk.lines {
                match line {
                    DiffLine::Addition {
                        inline_highlights, ..
                    }
                    | DiffLine::Deletion {
                        inline_highlights, ..
                    } => {
                        assert!(inline_highlights.is_empty());
                    }
                    DiffLine::Context { .. } => {}
                }
            }
        }
    }

    #[test]
    fn mode_b_structural_hunks() {
        let left = "fn a() {\n    work(foo);\n}\n";
        let right = "fn a() {\n    work(bar);\n}\n";

        let hunks = compute(&DiffAlgorithm::SyntaxAware, left, right, Path::new("x.rs"))
            .expect("structural diff");

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].before_lines, 1..2);
        assert_eq!(hunks[0].after_lines, 1..2);

        let old = line_ranges(left);
        let new = line_ranges(right);
        assert_eq!(row_texts(&hunks, left, &old, true), vec!["foo"]);
        assert_eq!(row_texts(&hunks, right, &new, false), vec!["bar"]);
    }

    #[test]
    fn mode_b_reindent_yields_no_hunks() {
        // A pure reindentation is structurally unchanged: difftastic-style
        // empty diff.
        let left = "fn a() {\n    work();\n}\n";
        let right = "    fn a() {\n        work();\n    }\n";

        let hunks = compute(&DiffAlgorithm::SyntaxAware, left, right, Path::new("x.rs"))
            .expect("structural diff");

        assert!(hunks.is_empty(), "reindent should be equal: {hunks:#?}");
    }

    #[test]
    fn mode_b_falls_back_to_plain_lines_on_broken_syntax() {
        let left = "fn [[[ = = {{{{\n((((\n";
        let right = "fn [[[ = = {{{{\n(((( x\n";

        let hunks = compute(&DiffAlgorithm::SyntaxAware, left, right, Path::new("x.rs"))
            .expect("fallback still yields hunks");

        // Fallback produced imara hunks (no crash, just plain rows).
        assert!(!hunks.is_empty());
    }

    #[test]
    fn unsupported_extension_diffs_without_highlights() {
        let left = "hello world\nsecond line\n";
        let right = "hello there\nsecond line\n";

        let hunks =
            compute(&DiffAlgorithm::Myers, left, right, Path::new("x.bogus")).expect("plain diff");

        assert!(!hunks.is_empty());
    }
}
