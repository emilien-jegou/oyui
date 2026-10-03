//! Hunk splitting and joining operations on FileDiff.

use crate::diff::{FileDiff, HunkMarker};
use std::path::PathBuf;

/// Splits a hunk at the given line index within the hunk.
pub fn split_hunk_at(diff: &mut FileDiff, hunk_idx: usize, split_idx: usize, marker: HunkMarker) {
    diff.split_hunk(hunk_idx, split_idx, marker);
}

/// Joins hunk at `hunk_idx` with the previous hunk.
pub fn join_hunk_at(
    diff: &mut FileDiff,
    tree_rw: &parking_lot::RwLock<crate::tree::FileTree>,
    path: &PathBuf,
    hunk_idx: usize,
    sync_staging_before_merge: bool,
) {
    let default = if sync_staging_before_merge {
        crate::diff::staging::is_file_staged_default(tree_rw, path)
    } else {
        false
    };
    diff.join_hunk(hunk_idx, sync_staging_before_merge, default);
}
