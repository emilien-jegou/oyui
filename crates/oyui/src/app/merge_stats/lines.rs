//! Line-level counting for [`super::merge_stats`].
//!
//! Prefers the staging state of a computed diff, and falls back to the
//! worker's stats cache. Nothing here touches the filesystem: this runs
//! while painting.

use crate::diff_cache::DiffCache;
use crate::tree::{StagingState, TreeNodeFile};

type SideCounts = super::SideCounts;

/// Walks a cached diff, attributing each line to the side it is staged on.
pub(super) fn count_staged_lines(
    diff: &crate::diff::FileDiff,
    f: &TreeNodeFile,
    left: &mut SideCounts,
    right: &mut SideCounts,
) {
    let default_staged = f.state == StagingState::Staged;
    let mut selection_idx = 0;

    for hunk in &diff.hunks {
        for diff_line in &hunk.lines {
            let staged = diff.line_selections.get(selection_idx, default_staged);
            selection_idx += 1;
            match diff_line {
                crate::diff::DiffLine::Addition { .. } => {
                    if staged {
                        left.3 += 1;
                    } else {
                        right.3 += 1;
                    }
                }
                crate::diff::DiffLine::Deletion { .. } => {
                    if staged {
                        left.4 += 1;
                    } else {
                        right.4 += 1;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Counts lines for a file whose diff has not been computed yet.
///
/// The worker stats every file in the tree, so a missing entry means the
/// batch has not landed yet, and a [`crate::diff::DiffStats::Binary`] entry
/// means the file has no countable lines. Either way there is nothing to add.
pub(super) fn count_stats_lines(
    f: &TreeNodeFile,
    cache: &DiffCache,
    left: &mut SideCounts,
    right: &mut SideCounts,
) {
    let stats = cache.stats.get(&f.path);
    let Some(crate::diff::DiffStats::Text {
        insertions,
        deletions,
    }) = stats.as_deref()
    else {
        return;
    };

    let target = if f.state == StagingState::Unstaged {
        right
    } else {
        left
    };
    target.3 += insertions;
    target.4 += deletions;
}
