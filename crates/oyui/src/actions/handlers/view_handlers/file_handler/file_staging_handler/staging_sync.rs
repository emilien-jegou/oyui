//! High-level staging operations that bridge FileDiff and FileTree.
//!
//! These are persistence operations — they take the tree lock and write
//! back — so they live in the app layer, not next to the pure `FileDiff`
//!(finding 3.5: diff logic kept total and lock-free).

use crate::diff::FileDiff;
use crate::tree::{FileTree, StagingState};
use parking_lot::RwLock;
use std::path::Path;

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

/// Inverts all staging selections for a text diff and updates the tree state.
pub fn invert_text_diff_staging(
    diff: &mut FileDiff,
    tree: &RwLock<crate::tree::FileTree>,
    path: &Path,
) {
    let default_staged = is_file_staged_default(tree, path);
    diff.invert_staging(default_staged);
    update_tree_staging_state(tree, path, diff, !default_staged);
}

/// Toggles staging for a hunk and updates the tree state.
pub fn toggle_stage_hunk_in_diff(
    tree_rw: &RwLock<crate::tree::FileTree>,
    path: &Path,
    diff: &mut FileDiff,
    hunk_idx: usize,
    expand: bool,
) {
    let default_staged = is_file_staged_default(tree_rw, path);
    diff.toggle_hunk(hunk_idx, expand, default_staged);
    update_tree_staging_state(tree_rw, path, diff, default_staged);
}
