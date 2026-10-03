//! Per-line staging selections for a FileDiff.

use crate::diff::{DiffLine, FileDiff};
use crate::tree::StagingState;

/// Returns true if a diff line can be staged/unstaged.
pub fn is_modifiable(line: &DiffLine) -> bool {
    matches!(line, DiffLine::Addition { .. } | DiffLine::Deletion { .. })
}

/// Per-line staging selections for a [`FileDiff`].
///
/// Owns the `Vec<bool>` backing store and guarantees that it is always
/// sized to the total number of hunk lines, with a configurable default
/// for unmaterialized entries.
#[derive(Debug, Clone, Default)]
pub struct LineSelections {
    inner: Vec<bool>,
}

impl LineSelections {
    /// Creates selections sized to `total_lines`, all initialized to `default`.
    pub fn new(total_lines: usize, default: bool) -> Self {
        Self {
            inner: vec![default; total_lines],
        }
    }

    /// Resizes to `total_lines` if needed, filling new entries with `default`.
    pub fn ensure_size(&mut self, total_lines: usize, default: bool) {
        if self.inner.len() != total_lines {
            self.inner.resize(total_lines, default);
        }
    }

    /// Returns the selection at `idx`, or `default` if out of bounds.
    pub fn get(&self, idx: usize, default: bool) -> bool {
        self.inner.get(idx).copied().unwrap_or(default)
    }

    /// Sets the selection at `idx` if in bounds.
    pub fn set(&mut self, idx: usize, val: bool) {
        if let Some(slot) = self.inner.get_mut(idx) {
            *slot = val;
        }
    }

    /// Returns the number of selection entries.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if there are no selection entries.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns true if all selections match the given value.
    pub fn all(&self, val: bool) -> bool {
        self.inner.iter().all(|&b| b == val)
    }

    /// Inverts all selections.
    pub fn invert(&mut self) {
        for b in &mut self.inner {
            *b = !*b;
        }
    }

    /// Computes the overall [`StagingState`] from selections.
    pub fn staging_state(&self, diff: &FileDiff, default: bool) -> StagingState {
        let mut has_staged = false;
        let mut has_unstaged = false;
        let mut idx = 0;

        for hunk in &diff.hunks {
            for line in &hunk.lines {
                if is_modifiable(line) {
                    if self.get(idx, default) {
                        has_staged = true;
                    } else {
                        has_unstaged = true;
                    }
                }
                idx += 1;
            }
        }

        if has_staged && has_unstaged {
            StagingState::PartiallyStaged
        } else if has_staged {
            StagingState::Staged
        } else {
            StagingState::Unstaged
        }
    }

    /// Returns the staged file content by applying selections to the diff.
    ///
    /// `default` is used for any selection entries that haven't been materialized.
    pub fn staged_content(&self, diff: &FileDiff, default: bool) -> String {
        let old_lines: Vec<&str> = diff.old_file_content.split('\n').collect();
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();
        let mut out = String::new();
        let mut current_old_line = 0;
        let mut selection_idx = 0;
        let mut first_line_written = false;

        for hunk in &diff.hunks {
            while current_old_line < hunk.before_lines.start {
                if current_old_line < old_lines.len() {
                    if first_line_written {
                        out.push('\n');
                    }
                    out.push_str(old_lines[current_old_line]);
                    first_line_written = true;
                }
                current_old_line += 1;
            }

            for diff_line in &hunk.lines {
                let is_staged = self.get(selection_idx, default);
                selection_idx += 1;

                match diff_line {
                    DiffLine::Context { old_line_idx, .. } => {
                        if *old_line_idx < old_lines.len() {
                            if first_line_written {
                                out.push('\n');
                            }
                            out.push_str(old_lines[*old_line_idx]);
                            first_line_written = true;
                        }
                        current_old_line = *old_line_idx + 1;
                    }
                    DiffLine::Deletion { old_line_idx, .. } => {
                        if !is_staged && *old_line_idx < old_lines.len() {
                            if first_line_written {
                                out.push('\n');
                            }
                            out.push_str(old_lines[*old_line_idx]);
                            first_line_written = true;
                        }
                        current_old_line = *old_line_idx + 1;
                    }
                    DiffLine::Addition { new_line_idx, .. } => {
                        if is_staged && *new_line_idx < new_lines.len() {
                            if first_line_written {
                                out.push('\n');
                            }
                            out.push_str(new_lines[*new_line_idx]);
                            first_line_written = true;
                        }
                    }
                }
            }
        }

        while current_old_line < old_lines.len() {
            if first_line_written {
                out.push('\n');
            }
            out.push_str(old_lines[current_old_line]);
            first_line_written = true;
            current_old_line += 1;
        }

        out
    }
}
