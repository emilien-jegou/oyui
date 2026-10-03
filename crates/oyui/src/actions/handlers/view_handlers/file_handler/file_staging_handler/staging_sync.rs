//! High-level staging operations that bridge FileDiff and FileTree.

use crate::diff::staging::{is_file_staged_default, update_tree_staging_state};
use crate::diff::FileDiff;
use parking_lot::RwLock;
use std::path::Path;

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
