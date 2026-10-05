//! Read-only introspection of the open file's diff and hunks.

use std::path::PathBuf;
use std::sync::Arc;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff::{DiffLine, DiffResult, DiffStats, FileDiff};

impl ViewFileInspectActionsHandler for AppActionsHandler {
    fn hunk_has(&self) -> bool {
        self.current_hunk().is_some()
    }

    fn hunk_index(&self) -> Option<u32> {
        self.current_hunk().map(|(i, _)| i as u32)
    }

    fn hunk_count(&self) -> u32 {
        self.open_diff()
            .and_then(|(_, r)| match r.as_ref() {
                DiffResult::Text(d) => Some(d.hunks.len() as u32),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn hunk_text(&self) -> String {
        self.current_hunk_text().unwrap_or_default().join("\n")
    }

    fn hunk_marker(&self) -> String {
        self.current_hunk()
            .and_then(|(i, d)| match d.as_ref() {
                DiffResult::Text(fd) => fd
                    .hunks
                    .get(i)
                    .map(|h| format!("{:?}", h.marker).to_lowercase()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn hunk_is_staged(&self) -> bool {
        let Some((hi, d)) = self.current_hunk() else {
            return false;
        };
        let DiffResult::Text(diff) = d.as_ref() else {
            return false;
        };
        let default = self.is_default_staged();
        let offset = diff.hunk_start_idx(hi);
        diff.is_hunk_fully_staged(hi, offset, default)
    }

    fn diff_text(&self) -> String {
        self.open_diff()
            .and_then(|(_, r)| match r.as_ref() {
                DiffResult::Text(d) => Some(unified_diff(d)),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn new_text(&self) -> String {
        self.open_diff()
            .and_then(|(_, r)| match r.as_ref() {
                DiffResult::Text(d) => Some(d.new_file_content.to_string()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn old_text(&self) -> String {
        self.open_diff()
            .and_then(|(_, r)| match r.as_ref() {
                DiffResult::Text(d) => Some(d.old_file_content.to_string()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn staged_text(&self) -> String {
        self.open_diff()
            .and_then(|(_, r)| match r.as_ref() {
                DiffResult::Text(d) => Some(d.staged_content(self.is_default_staged())),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn stats(&self) -> String {
        let Some((path, _)) = self.open_diff() else {
            return String::new();
        };
        format_stats(self.cache.stats.get(&path).as_deref())
    }

    fn is_binary(&self) -> bool {
        self.open_diff()
            .is_some_and(|(_, r)| matches!(r.as_ref(), DiffResult::Binary { .. }))
    }
}

impl AppActionsHandler {
    /// Returns the open file's path and cached diff result.
    fn open_diff(&self) -> Option<(PathBuf, Arc<DiffResult>)> {
        let path = self.ui.lock().file_view.current_path.clone()?;
        let diff = self.cache.diffs.get(&path)?;
        Some((path, diff))
    }

    /// Returns the hunk under the cursor and the open file's diff.
    fn current_hunk(&self) -> Option<(usize, Arc<DiffResult>)> {
        let (path, diff) = self.open_diff()?;
        let DiffResult::Text(file_diff) = diff.as_ref() else {
            return None;
        };
        let ui = self.ui.lock();
        let row = ui
            .file_view
            .scroll_states
            .get(&path)
            .and_then(|s| s.selected())
            .unwrap_or(0);
        let hi = ui
            .file_view
            .row_to_hunk(&path)
            .and_then(|m| m.get(row).copied().flatten())?;
        if hi >= file_diff.hunks.len() {
            return None;
        }
        Some((hi, Arc::clone(&diff)))
    }

    /// Returns the newline-joined text lines of the hunk under the cursor.
    fn current_hunk_text(&self) -> Option<Vec<String>> {
        let (hi, diff) = self.current_hunk()?;
        let DiffResult::Text(file_diff) = diff.as_ref() else {
            return None;
        };
        hunk_lines(file_diff, hi)
    }

    /// Returns whether the open file's default staging state is "staged".
    fn is_default_staged(&self) -> bool {
        let Some(path) = self.ui.lock().file_view.current_path.clone() else {
            return false;
        };
        self.tree.read().get_file_state(&path) == Some(crate::tree::StagingState::Staged)
    }
}

/// Joins a hunk's rendered line text (new side for context/additions).
pub(crate) fn hunk_lines(diff: &FileDiff, hi: usize) -> Option<Vec<String>> {
    let hunk = diff.hunks.get(hi)?;
    let new_lines: Vec<&str> = diff.new_file_content.lines().collect();
    let old_lines: Vec<&str> = diff.old_file_content.lines().collect();
    Some(
        hunk.lines
            .iter()
            .map(|line| match line {
                DiffLine::Context { new_line_idx, .. } => {
                    new_lines.get(*new_line_idx).copied().unwrap_or("")
                }
                DiffLine::Deletion { old_line_idx, .. } => {
                    old_lines.get(*old_line_idx).copied().unwrap_or("")
                }
                DiffLine::Addition { new_line_idx, .. } => {
                    new_lines.get(*new_line_idx).copied().unwrap_or("")
                }
            })
            .map(str::to_string)
            .collect(),
    )
}

/// Renders a minimal unified diff for a file.
pub(crate) fn unified_diff(diff: &FileDiff) -> String {
    let new_lines: Vec<&str> = diff.new_file_content.lines().collect();
    let old_lines: Vec<&str> = diff.old_file_content.lines().collect();
    let mut out = String::new();
    for hunk in &diff.hunks {
        let old_len = hunk
            .before_lines
            .end
            .saturating_sub(hunk.before_lines.start);
        let new_len = hunk.after_lines.end.saturating_sub(hunk.after_lines.start);
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.before_lines.start + 1,
            old_len,
            hunk.after_lines.start + 1,
            new_len
        ));
        for line in &hunk.lines {
            match line {
                DiffLine::Context { new_line_idx, .. } => {
                    out.push(' ');
                    out.push_str(new_lines.get(*new_line_idx).copied().unwrap_or(""));
                }
                DiffLine::Deletion { old_line_idx, .. } => {
                    out.push('-');
                    out.push_str(old_lines.get(*old_line_idx).copied().unwrap_or(""));
                }
                DiffLine::Addition { new_line_idx, .. } => {
                    out.push('+');
                    out.push_str(new_lines.get(*new_line_idx).copied().unwrap_or(""));
                }
            }
            out.push('\n');
        }
    }
    out
}

/// Formats cached diff stats as `+N -M` (or a binary marker).
pub(crate) fn format_stats(stats: Option<&DiffStats>) -> String {
    match stats {
        Some(DiffStats::Text {
            insertions,
            deletions,
        }) => format!("+{insertions} -{deletions}"),
        Some(DiffStats::Binary { bytes }) => format!("binary {bytes}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, Hunk, LineSelections};

    fn fixture() -> FileDiff {
        FileDiff {
            old_file_content: Arc::from("a\nb\nc"),
            new_file_content: Arc::from("a\nB\nc\nd"),
            hunks: vec![Hunk {
                before_lines: 0..3,
                after_lines: 0..4,
                lines: vec![
                    DiffLine::Context {
                        old_line_idx: 0,
                        new_line_idx: 0,
                    },
                    DiffLine::Deletion {
                        old_line_idx: 1,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 1,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Context {
                        old_line_idx: 2,
                        new_line_idx: 2,
                    },
                ],
                marker: Default::default(),
            }],
            line_selections: LineSelections::default(),
        }
    }

    #[test]
    fn hunk_lines_uses_the_new_side_for_context_and_additions() {
        let lines = hunk_lines(&fixture(), 0).expect("hunk exists");
        assert_eq!(lines, vec!["a", "b", "B", "c"]);
    }

    #[test]
    fn hunk_lines_out_of_range_is_none() {
        assert!(hunk_lines(&fixture(), 9).is_none());
    }

    #[test]
    fn unified_diff_marks_each_side() {
        let text = unified_diff(&fixture());
        assert!(text.starts_with("@@ -1,3 +1,4 @@"), "{text}");
        assert!(text.contains("\n-b\n"), "{text}");
        assert!(text.contains("\n+B\n"), "{text}");
    }

    #[test]
    fn format_stats_handles_text_missing_and_binary() {
        let stats = DiffStats::Text {
            insertions: 2,
            deletions: 3,
        };
        assert_eq!(format_stats(Some(&stats)), "+2 -3");
        assert_eq!(format_stats(None), "");
        assert_eq!(
            format_stats(Some(&DiffStats::Binary { bytes: 42 })),
            "binary 42"
        );
    }
}
