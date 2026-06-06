use super::utils::*;
use crate::diff::{DiffLine, FileDiff, HunkMarker};
use crate::tree::{FileTree, StagingState, TreeNode, TreeNodeFile};
use parking_lot::RwLock;
use std::path::PathBuf;

fn find_file_mut<'a>(nodes: &'a mut [TreeNode], path: &PathBuf) -> Option<&'a mut TreeNodeFile> {
    for node in nodes {
        match node {
            TreeNode::File(file) if file.path == *path => return Some(file),
            TreeNode::Directory(dir) => {
                if let Some(f) = find_file_mut(&mut dir.children, path) {
                    return Some(f);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn update_tree_staging_state(
    tree_rw: &RwLock<FileTree>,
    path: &PathBuf,
    diff: &FileDiff,
    default_staged: bool,
) {
    let new_state = compute_diff_staging_state(diff, default_staged);
    let mut tree = tree_rw.write();
    if let Some(file) = find_file_mut(&mut tree.nodes, path) {
        file.state = new_state;
    }
}

pub fn toggle_binary_file_staging_state(tree_rw: &RwLock<FileTree>, path: &PathBuf) {
    let mut tree = tree_rw.write();
    if let Some(file) = find_file_mut(&mut tree.nodes, path) {
        file.state = file.state.toggle();
    }
}

fn compute_diff_staging_state(diff: &FileDiff, default_staged: bool) -> StagingState {
    let mut has_staged = false;
    let mut has_unstaged = false;
    let mut current_idx = 0;

    for h in &diff.hunks {
        for line in &h.lines {
            if matches!(line, DiffLine::Addition { .. } | DiffLine::Deletion { .. }) {
                let is_staged = diff
                    .line_selections
                    .get(current_idx)
                    .copied()
                    .unwrap_or(default_staged);
                if is_staged {
                    has_staged = true;
                } else {
                    has_unstaged = true;
                }
            }
            current_idx += 1;
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

pub fn invert_text_diff_staging(diff: &mut FileDiff, tree: &RwLock<FileTree>, path: &PathBuf) {
    let default_staged = is_file_staged_default(tree, path);
    ensure_selection_size(diff, default_staged);

    diff.line_selections.iter_mut().for_each(|b| *b = !*b);

    update_tree_staging_state(tree, path, diff, !default_staged);
}

pub fn toggle_stage_hunk_in_diff(
    tree_rw: &RwLock<FileTree>,
    path: &PathBuf,
    diff: &mut FileDiff,
    hunk_idx: usize,
    expand: bool,
) {
    let default_staged = is_file_staged_default(tree_rw, path);
    ensure_selection_size(diff, default_staged);

    let (left, right) = calculate_expanded_bounds(diff, hunk_idx, expand);
    let all_staged = all_stageable_staged(diff, left, right, expand, default_staged);
    apply_staging_state(diff, left, right, expand, !all_staged);

    update_tree_staging_state(tree_rw, path, diff, default_staged);
}

fn calculate_expanded_bounds(diff: &FileDiff, hunk_idx: usize, expand: bool) -> (usize, usize) {
    let mut left = hunk_idx;
    let mut right = hunk_idx;

    if !expand {
        return (left, right);
    }

    while left > 0
        && are_hunks_contiguous(diff, left - 1, left)
        && diff.hunks[left].marker != HunkMarker::HunkSplit
    {
        left -= 1;
    }

    while right + 1 < diff.hunks.len()
        && are_hunks_contiguous(diff, right, right + 1)
        && diff.hunks[right + 1].marker != HunkMarker::HunkSplit
    {
        right += 1;
    }

    (left, right)
}

fn all_stageable_staged(
    diff: &FileDiff,
    left: usize,
    right: usize,
    expand: bool,
    default: bool,
) -> bool {
    let mut offset = get_hunk_start_line_idx(diff, left);
    (left..=right).all(|idx| {
        let len = diff.hunks[idx].lines.len();
        let result = (expand && diff.hunks[idx].marker == HunkMarker::LineToggle)
            || is_hunk_fully_staged(diff, idx, offset, default);
        offset += len;
        result
    })
}

fn apply_staging_state(
    diff: &mut FileDiff,
    left: usize,
    right: usize,
    expand: bool,
    new_state: bool,
) {
    let mut offset = get_hunk_start_line_idx(diff, left);
    for idx in left..=right {
        let len = diff.hunks[idx].lines.len();
        if !(expand && diff.hunks[idx].marker == HunkMarker::LineToggle) {
            set_hunk_line_staging(diff, idx, offset, new_state);
        }
        offset += len;
    }
}

fn is_hunk_fully_staged(
    diff: &FileDiff,
    hunk_idx: usize,
    offset: usize,
    default_staged: bool,
) -> bool {
    diff.hunks[hunk_idx]
        .lines
        .iter()
        .enumerate()
        .all(|(j, line)| {
            !is_modifiable(line)
                || diff
                    .line_selections
                    .get(offset + j)
                    .copied()
                    .unwrap_or(default_staged)
        })
}
