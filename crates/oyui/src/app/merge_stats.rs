//! Merge statistics derived from the file tree and the diff cache.
//!
//! Everything here runs around the paint path, so it only ever takes read
//! locks: refresh derived state with [`sync_line_selections`] *before*
//! rendering, never from inside it.

use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState, TreeNodeFile};
use parking_lot::RwLock;
use std::sync::Arc;

type SideCounts = (usize, usize, usize, usize, usize);

/// Aligns cached line selections with each entry's staging state.
///
/// Call this when entering a mode that reports staging counts, not while
/// painting: rendering must not mutate shared state.
pub fn sync_line_selections(tree: &Arc<RwLock<FileTree>>, cache: &DiffCache) {
    let tree = tree.read();
    for f in tree.files() {
        if f.state == StagingState::Staged || f.state == StagingState::Unstaged {
            let staged = f.state == StagingState::Staged;
            cache.diffs.update(&f.path, |diff_result| {
                if let crate::diff::DiffResult::Text(diff) = diff_result {
                    let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
                    diff.line_selections.ensure_size(total_lines, staged);
                }
            });
        }
    }
}

mod lines;

#[cfg(test)]
mod tests;

use lines::{count_staged_lines, count_stats_lines};

/// Counts added, deleted and modified files in `tree`.
pub fn diff_summary(tree: &Arc<RwLock<FileTree>>) -> (usize, usize, usize) {
    let tree = tree.read();
    let (mut a, mut d, mut m) = (0, 0, 0);
    for f in tree.files() {
        if f.left_path.is_none() {
            a += 1;
        } else if f.right_path.is_none() {
            d += 1;
        } else {
            m += 1;
        }
    }
    (a, d, m)
}

/// Per-side staged and unstaged counts for the merge confirmation window.
pub fn merge_stats(tree: &Arc<RwLock<FileTree>>, cache: &DiffCache) -> (SideCounts, SideCounts) {
    let tree = tree.read();
    let mut left = (0, 0, 0, 0, 0);
    let mut right = (0, 0, 0, 0, 0);
    for f in tree.files() {
        count_file(f, cache, &mut left, &mut right);
    }
    (left, right)
}

/// Counts one file's staged/unstaged rows into the left and right buckets.
///
/// `left` is the staged side, `right` the unstaged side. Bucket order is:
/// added files, deleted files, modified files, line additions, line
/// deletions.
fn count_file(f: &TreeNodeFile, cache: &DiffCache, left: &mut SideCounts, right: &mut SideCounts) {
    let is_added = f.left_path.is_none();
    let is_deleted = f.right_path.is_none();

    if f.state == StagingState::Staged || f.state == StagingState::PartiallyStaged {
        bucket(is_added, is_deleted, left);
    }

    if f.state == StagingState::Unstaged || f.state == StagingState::PartiallyStaged {
        bucket(is_added, is_deleted, right);
    }

    match cache.diffs.get(&f.path).as_deref() {
        Some(crate::diff::DiffResult::Text(diff)) => count_staged_lines(diff, f, left, right),
        _ => count_stats_lines(f, cache, left, right),
    }
}

/// Adds `f` to `counts` as added, deleted or modified.
fn bucket(is_added: bool, is_deleted: bool, counts: &mut SideCounts) {
    if is_added {
        counts.0 += 1;
    } else if is_deleted {
        counts.1 += 1;
    } else {
        counts.2 += 1;
    }
}
