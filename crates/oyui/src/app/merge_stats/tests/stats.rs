//! Line counts sourced from the stats cache, with no filesystem fallback.

use super::tree_of;
use crate::app::merge_stats::merge_stats;
use crate::diff::DiffStats;
use crate::diff_cache::DiffCache;
use crate::tree::StagingState;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn line_counts_come_from_the_stats_cache_not_the_disk() {
    let tree = tree_of(&[("new.txt", false, true, StagingState::Unstaged)]);
    let cache = DiffCache::default();
    cache.stats.set(
        PathBuf::from("new.txt"),
        Arc::new(DiffStats::Text {
            insertions: 7,
            deletions: 2,
        }),
        cache.stats.generation(),
    );

    let (left, right) = merge_stats(&tree, &cache);

    assert_eq!(left, (0, 0, 0, 0, 0));
    assert_eq!(
        right,
        (1, 0, 0, 7, 2),
        "the paths do not exist, so these counts cannot come from reading them"
    );
}

#[test]
fn binary_stats_contribute_no_lines() {
    let tree = tree_of(&[("blob.bin", false, true, StagingState::Unstaged)]);
    let cache = DiffCache::default();
    cache.stats.set(
        PathBuf::from("blob.bin"),
        Arc::new(DiffStats::Binary { bytes: 4096 }),
        cache.stats.generation(),
    );

    let (left, right) = merge_stats(&tree, &cache);

    assert_eq!(left, (0, 0, 0, 0, 0));
    assert_eq!(
        right,
        (1, 0, 0, 0, 0),
        "a binary file has no lines; it must not fall back to reading the file"
    );
}

#[test]
fn missing_stats_are_counted_as_zero_lines() {
    let tree = tree_of(&[
        ("absent.txt", false, true, StagingState::Staged),
        ("gone.txt", true, false, StagingState::Unstaged),
    ]);
    let cache = DiffCache::default();

    let (left, right) = merge_stats(&tree, &cache);

    assert_eq!(left, (1, 0, 0, 0, 0), "added file, staged side");
    assert_eq!(right, (0, 1, 0, 0, 0), "deleted file, unstaged side");
}
