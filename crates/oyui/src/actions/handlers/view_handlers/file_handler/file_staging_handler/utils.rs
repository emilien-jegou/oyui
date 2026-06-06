use crate::diff::{DiffLine, FileDiff, HunkMarker};
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::path::PathBuf;

pub fn is_modifiable(line: &DiffLine) -> bool {
    matches!(line, DiffLine::Addition { .. } | DiffLine::Deletion { .. })
}

pub fn is_file_staged_default(tree_rw: &RwLock<FileTree>, path: &PathBuf) -> bool {
    tree_rw
        .read()
        .get_file_state(path)
        .unwrap_or(crate::tree::StagingState::Unstaged)
        == crate::tree::StagingState::Staged
}

/// Returns true if hunk `a` ends exactly where hunk `b` begins (line-contiguous).
pub fn are_hunks_contiguous(diff: &FileDiff, a: usize, b: usize) -> bool {
    diff.hunks[a].after_lines.end == diff.hunks[b].after_lines.start
}

pub fn ensure_selection_size(diff: &mut FileDiff, default_staged: bool) {
    let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
    if diff.line_selections.len() != total_lines {
        diff.line_selections.resize(total_lines, default_staged);
    }
}

pub fn get_hunk_start_line_idx(diff: &FileDiff, hunk_idx: usize) -> usize {
    diff.hunks
        .iter()
        .take(hunk_idx)
        .map(|h| h.lines.len())
        .sum()
}

pub fn find_parent_staging_status(diff: &FileDiff, hunk_idx: usize, default_staged: bool) -> bool {
    if hunk_idx == 0 {
        return default_staged;
    }

    let mut upper_idx = hunk_idx - 1;
    while upper_idx > 0 && diff.hunks[upper_idx].marker == HunkMarker::LineToggle {
        upper_idx -= 1;
    }

    if diff.hunks[upper_idx].marker == HunkMarker::LineToggle {
        return default_staged;
    }

    let start_line = get_hunk_start_line_idx(diff, upper_idx);
    for (i, line) in diff.hunks[upper_idx].lines.iter().enumerate() {
        if is_modifiable(line) {
            return diff
                .line_selections
                .get(start_line + i)
                .copied()
                .unwrap_or(default_staged);
        }
    }

    default_staged
}

pub fn find_contiguous_range(diff: &FileDiff, hunk_idx: usize) -> std::ops::Range<usize> {
    let mut end_idx = hunk_idx;
    while end_idx + 1 < diff.hunks.len()
        && are_hunks_contiguous(diff, end_idx, end_idx + 1)
        && diff.hunks[end_idx + 1].marker != HunkMarker::HunkSplit
    {
        end_idx += 1;
    }
    hunk_idx..end_idx + 1
}

pub fn set_hunk_line_staging(diff: &mut FileDiff, hunk_idx: usize, offset: usize, staged: bool) {
    for (i, line) in diff.hunks[hunk_idx].lines.iter().enumerate() {
        if is_modifiable(line) && offset + i < diff.line_selections.len() {
            diff.line_selections[offset + i] = staged;
        }
    }
}

pub fn sync_contiguous_lines_to_parent_status(
    diff: &mut FileDiff,
    hunk_idx: usize,
    parent_status: bool,
) {
    let range = find_contiguous_range(diff, hunk_idx);
    let mut current_offset = get_hunk_start_line_idx(diff, hunk_idx);

    for idx in range {
        let lines_len = diff.hunks[idx].lines.len();
        if diff.hunks[idx].marker != HunkMarker::LineToggle {
            set_hunk_line_staging(diff, idx, current_offset, parent_status);
        }
        current_offset += lines_len;
    }
}
