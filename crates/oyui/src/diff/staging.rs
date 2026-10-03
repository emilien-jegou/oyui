//! Staging operations on FileDiff.

use crate::diff::line_selections::is_modifiable;
use crate::diff::{DiffLine, FileDiff, Hunk, HunkMarker};
use crate::tree::{FileTree, StagingState};
use parking_lot::RwLock;
use std::ops::Range;
use std::path::Path;

impl FileDiff {
    /// Returns the start line index for a hunk (sum of lines in preceding hunks).
    pub fn hunk_start_idx(&self, hunk_idx: usize) -> usize {
        self.hunks
            .iter()
            .take(hunk_idx)
            .map(|h| h.lines.len())
            .sum()
    }

    /// Returns true if hunk `a` ends exactly where hunk `b` begins.
    pub fn hunks_contiguous(&self, a: usize, b: usize) -> bool {
        self.hunks[a].after_lines.end == self.hunks[b].after_lines.start
    }

    /// Ensures `line_selections` is sized to the total number of hunk lines.
    pub fn ensure_selection_size(&mut self, default: bool) {
        let total_lines: usize = self.hunks.iter().map(|h| h.lines.len()).sum();
        self.line_selections.ensure_size(total_lines, default);
    }

    /// Toggles staging for a hunk, optionally expanding to contiguous hunks.
    pub fn toggle_hunk(&mut self, hunk_idx: usize, expand: bool, default: bool) {
        // Fresh diffs start with empty selections; size before writing or the
        // writes below are silently dropped and the toggle becomes a no-op.
        self.ensure_selection_size(default);
        let (left, right) = self.expanded_bounds(hunk_idx, expand);
        let all_staged = self.all_stageable_staged(left, right, expand, default);
        self.apply_staging_to_range(left, right, expand, !all_staged);
    }

    /// Inverts all staging selections.
    pub fn invert_staging(&mut self, default: bool) {
        self.ensure_selection_size(default);
        self.line_selections.invert();
    }

    /// Computes the overall [`StagingState`] from selections.
    pub fn staging_state(&self, default: bool) -> StagingState {
        self.line_selections.staging_state(self, default)
    }

    /// Returns the staged file content by applying selections to the diff.
    pub fn staged_content(&self, default: bool) -> String {
        self.line_selections.staged_content(self, default)
    }

    /// Splits a hunk at the given line index within the hunk.
    pub fn split_hunk(&mut self, hunk_idx: usize, split_idx: usize, marker: HunkMarker) {
        if hunk_idx >= self.hunks.len() {
            return;
        }
        let hunk = &self.hunks[hunk_idx];
        if split_idx == 0 || split_idx >= hunk.lines.len() {
            return;
        }

        let mut hunk_a = hunk.clone();
        let mut hunk_b = hunk.clone();

        hunk_a.lines = hunk.lines[..split_idx].to_vec();
        hunk_b.lines = hunk.lines[split_idx..].to_vec();

        let (old_end, new_end) = calculate_hunk_ranges(&hunk_a);

        hunk_a.after_lines = hunk.after_lines.start..new_end;
        hunk_a.before_lines = hunk.before_lines.start..old_end;

        hunk_b.after_lines = new_end..hunk.after_lines.end;
        hunk_b.before_lines = old_end..hunk.before_lines.end;
        hunk_b.marker = marker;

        self.hunks.remove(hunk_idx);
        self.hunks.insert(hunk_idx, hunk_b);
        self.hunks.insert(hunk_idx, hunk_a);
    }

    /// Joins hunk at `hunk_idx` with the previous hunk.
    pub fn join_hunk(&mut self, hunk_idx: usize, sync_staging: bool, default: bool) {
        if hunk_idx == 0 || hunk_idx >= self.hunks.len() {
            return;
        }

        if !self.hunks_contiguous(hunk_idx - 1, hunk_idx) {
            return;
        }

        if sync_staging {
            self.ensure_selection_size(default);
            let parent_status = self.parent_staging_status(hunk_idx, default);
            self.sync_contiguous_to_parent(hunk_idx, parent_status);
        }

        self.perform_merge(hunk_idx);
    }

    /// Calculates expanded bounds for hunk toggling.
    pub fn expanded_bounds(&self, hunk_idx: usize, expand: bool) -> (usize, usize) {
        let mut left = hunk_idx;
        let mut right = hunk_idx;

        if !expand {
            return (left, right);
        }

        while left > 0
            && self.hunks_contiguous(left - 1, left)
            && self.hunks[left].marker != HunkMarker::HunkSplit
        {
            left -= 1;
        }

        while right + 1 < self.hunks.len()
            && self.hunks_contiguous(right, right + 1)
            && self.hunks[right + 1].marker != HunkMarker::HunkSplit
        {
            right += 1;
        }

        (left, right)
    }

    /// Checks if all stageable hunks in range are staged.
    pub fn all_stageable_staged(
        &self,
        left: usize,
        right: usize,
        expand: bool,
        default: bool,
    ) -> bool {
        let mut offset = self.hunk_start_idx(left);
        (left..=right).all(|idx| {
            let len = self.hunks[idx].lines.len();
            let result = (expand && self.hunks[idx].marker == HunkMarker::LineToggle)
                || self.is_hunk_fully_staged(idx, offset, default);
            offset += len;
            result
        })
    }

    /// Applies staging state to a range of hunks.
    pub fn apply_staging_to_range(
        &mut self,
        left: usize,
        right: usize,
        expand: bool,
        new_state: bool,
    ) {
        let mut offset = self.hunk_start_idx(left);
        for idx in left..=right {
            let len = self.hunks[idx].lines.len();
            if !(expand && self.hunks[idx].marker == HunkMarker::LineToggle) {
                self.set_hunk_staging(idx, offset, new_state);
            }
            offset += len;
        }
    }

    /// Checks if a hunk is fully staged.
    pub fn is_hunk_fully_staged(&self, hunk_idx: usize, offset: usize, default: bool) -> bool {
        self.hunks[hunk_idx]
            .lines
            .iter()
            .enumerate()
            .all(|(j, line)| !is_modifiable(line) || self.line_selections.get(offset + j, default))
    }

    /// Returns the parent staging status for a hunk.
    pub fn parent_staging_status(&self, hunk_idx: usize, default: bool) -> bool {
        if hunk_idx == 0 {
            return default;
        }

        let mut upper_idx = hunk_idx - 1;
        while upper_idx > 0 && self.hunks[upper_idx].marker == HunkMarker::LineToggle {
            upper_idx -= 1;
        }

        if self.hunks[upper_idx].marker == HunkMarker::LineToggle {
            return default;
        }

        let start_line = self.hunk_start_idx(upper_idx);
        for (i, line) in self.hunks[upper_idx].lines.iter().enumerate() {
            if is_modifiable(line) {
                return self.line_selections.get(start_line + i, default);
            }
        }

        default
    }

    /// Returns the contiguous range starting from a hunk.
    pub fn contiguous_range(&self, hunk_idx: usize) -> Range<usize> {
        let mut end_idx = hunk_idx;
        while end_idx + 1 < self.hunks.len()
            && self.hunks_contiguous(end_idx, end_idx + 1)
            && self.hunks[end_idx + 1].marker != HunkMarker::HunkSplit
        {
            end_idx += 1;
        }
        hunk_idx..end_idx + 1
    }

    /// Sets staging for all modifiable lines in a hunk.
    pub fn set_hunk_staging(&mut self, hunk_idx: usize, offset: usize, staged: bool) {
        for (i, line) in self.hunks[hunk_idx].lines.iter().enumerate() {
            if is_modifiable(line) {
                self.line_selections.set(offset + i, staged);
            }
        }
    }

    /// Syncs contiguous lines to parent status.
    pub fn sync_contiguous_to_parent(&mut self, hunk_idx: usize, parent_status: bool) {
        let range = self.contiguous_range(hunk_idx);
        let mut current_offset = self.hunk_start_idx(hunk_idx);

        for idx in range {
            let lines_len = self.hunks[idx].lines.len();
            if self.hunks[idx].marker != HunkMarker::LineToggle {
                self.set_hunk_staging(idx, current_offset, parent_status);
            }
            current_offset += lines_len;
        }
    }

    fn perform_merge(&mut self, hunk_idx: usize) {
        let prev_hunk = self.hunks[hunk_idx - 1].clone();
        let curr_hunk = self.hunks[hunk_idx].clone();

        let mut merged = prev_hunk.clone();
        merged.marker = if prev_hunk.marker == HunkMarker::LineToggle {
            curr_hunk.marker
        } else {
            prev_hunk.marker
        };
        merged.lines.extend(curr_hunk.lines);
        merged.before_lines = prev_hunk.before_lines.start..curr_hunk.before_lines.end;
        merged.after_lines = prev_hunk.after_lines.start..curr_hunk.after_lines.end;

        self.hunks.remove(hunk_idx);
        self.hunks.remove(hunk_idx - 1);
        self.hunks.insert(hunk_idx - 1, merged);
    }
}

fn calculate_hunk_ranges(hunk: &Hunk) -> (usize, usize) {
    let mut old_idx = hunk.before_lines.start;
    let mut new_idx = hunk.after_lines.start;
    for line in &hunk.lines {
        match line {
            DiffLine::Context { .. } => {
                old_idx += 1;
                new_idx += 1;
            }
            DiffLine::Deletion { .. } => {
                old_idx += 1;
            }
            DiffLine::Addition { .. } => {
                new_idx += 1;
            }
        }
    }
    (old_idx, new_idx)
}

/// Returns true if the file's default staging state is "staged".
pub fn is_file_staged_default(tree_rw: &RwLock<FileTree>, path: &Path) -> bool {
    tree_rw
        .read()
        .get_file_state(path)
        .unwrap_or(StagingState::Unstaged)
        == StagingState::Staged
}

/// Updates the tree's staging state for a file based on its diff selections.
pub fn update_tree_staging_state(
    tree_rw: &RwLock<FileTree>,
    path: &Path,
    diff: &FileDiff,
    default: bool,
) {
    let new_state = diff.staging_state(default);
    let mut tree = tree_rw.write();
    if let Some(file) = tree.file_mut(path) {
        file.state = new_state;
    }
}

/// Toggles the staging state for a binary file.
pub fn toggle_binary_file_staging_state(tree_rw: &RwLock<FileTree>, path: &Path) {
    let mut tree = tree_rw.write();
    if let Some(file) = tree.file_mut(path) {
        file.state = file.state.toggle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, FileDiff, Hunk, LineSelections};
    use std::sync::Arc;

    /// Fresh diffs ship with empty `line_selections`; toggling a hunk must
    /// materialize them or `space` silently does nothing.
    #[test]
    fn toggle_hunk_materializes_empty_selections() {
        let mut diff = FileDiff {
            old_file_content: Arc::from(""),
            new_file_content: Arc::from("x"),
            hunks: vec![Hunk {
                before_lines: 0..0,
                after_lines: 0..1,
                lines: vec![DiffLine::Addition {
                    new_line_idx: 0,
                    inline_highlights: Vec::new(),
                }],
                marker: HunkMarker::None,
            }],
            line_selections: LineSelections::default(),
        };

        diff.toggle_hunk(0, true, false);

        assert_eq!(
            diff.staging_state(false),
            StagingState::Staged,
            "toggling must stage the hunk, not drop the write"
        );
    }
}
