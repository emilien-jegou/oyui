//! Unit tests for [`super::merge_stats`].

mod stats;

use super::{merge_stats, sync_line_selections};
use crate::diff::{DiffLine, DiffResult, FileDiff, Hunk, HunkMarker, LineSelections};
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Builds a tree from `(name, left_exists, right_exists, state)` rows.
///
/// Every path is deliberately under `missing/`, so a count that requires
/// reading a file would return zero rather than a real number.
pub(super) fn tree_of(rows: &[(&str, bool, bool, StagingState)]) -> Arc<RwLock<FileTree>> {
    let mut tree = FileTree::default();
    let states: HashMap<&str, StagingState> = rows.iter().map(|(n, _, _, s)| (*n, *s)).collect();
    for (name, has_left, has_right, _) in rows {
        let left = has_left.then(|| PathBuf::from(format!("missing/{name}")));
        let right = has_right.then(|| PathBuf::from(format!("missing/{name}")));
        tree.insert_file(PathBuf::from(*name), left, right);
    }
    for f in tree.files_mut() {
        f.state = states[f.name.as_str()];
    }
    Arc::new(RwLock::new(tree))
}

/// A single-hunk diff over the given ordered lines, with unsized selections.
pub(super) fn file_diff(lines: Vec<DiffLine>) -> FileDiff {
    FileDiff {
        old_file_content: Arc::from(""),
        new_file_content: Arc::from(""),
        hunks: vec![Hunk {
            before_lines: 0..0,
            after_lines: 0..0,
            marker: HunkMarker::None,
            lines,
        }],
        line_selections: LineSelections::default(),
    }
}

pub(super) fn context() -> DiffLine {
    DiffLine::Context {
        old_line_idx: 0,
        new_line_idx: 0,
    }
}

pub(super) fn addition(idx: usize) -> DiffLine {
    DiffLine::Addition {
        new_line_idx: idx,
        inline_highlights: vec![],
    }
}

pub(super) fn deletion(idx: usize) -> DiffLine {
    DiffLine::Deletion {
        old_line_idx: idx,
        inline_highlights: vec![],
    }
}

#[test]
fn files_are_bucketed_by_their_staging_state() {
    let tree = tree_of(&[
        ("staged.txt", true, true, StagingState::Staged),
        ("unstaged.txt", true, true, StagingState::Unstaged),
        ("split.txt", true, true, StagingState::PartiallyStaged),
        ("added.txt", false, true, StagingState::Unstaged),
        ("deleted.txt", true, false, StagingState::Staged),
    ]);
    let cache = DiffCache::default();

    let (left, right) = merge_stats(&tree, &cache);

    assert_eq!(left, (0, 1, 2, 0, 0), "staged: 1 deleted, 2 modified");
    assert_eq!(right, (1, 0, 2, 0, 0), "unstaged: 1 added, 2 modified");
}

#[test]
fn computed_diffs_attribute_lines_to_their_own_side() {
    let tree = tree_of(&[
        ("staged.txt", true, true, StagingState::Staged),
        ("unstaged.txt", true, true, StagingState::Unstaged),
    ]);
    let cache = DiffCache::default();
    cache.diffs.set(
        PathBuf::from("staged.txt"),
        Arc::new(DiffResult::Text(file_diff(vec![context(), addition(1)]))),
        cache.diffs.generation(),
    );
    cache.diffs.set(
        PathBuf::from("unstaged.txt"),
        Arc::new(DiffResult::Text(file_diff(vec![
            addition(1),
            addition(2),
            addition(3),
            deletion(4),
        ]))),
        cache.diffs.generation(),
    );

    let (left, right) = merge_stats(&tree, &cache);

    assert_eq!(left, (0, 0, 1, 1, 0), "staged.txt: 1 addition");
    assert_eq!(
        right,
        (0, 0, 1, 3, 1),
        "unstaged.txt: 3 additions, 1 deletion"
    );
}

#[test]
fn sync_sizes_line_selections_to_the_diff() {
    let tree = tree_of(&[("f.txt", true, true, StagingState::Staged)]);
    let cache = DiffCache::default();
    let path = PathBuf::from("f.txt");
    let diff = file_diff(vec![context(), addition(1), deletion(1)]);

    // `full_diff` builds diffs with `LineSelections::default()`, i.e. empty.
    assert!(!diff.line_selections.get(2, false), "starts unsized");

    cache.diffs.set(
        path.clone(),
        Arc::new(DiffResult::Text(diff)),
        cache.diffs.generation(),
    );
    sync_line_selections(&tree, &cache);

    match cache.diffs.get(&path).as_deref() {
        Some(DiffResult::Text(d)) => assert!(
            d.line_selections.get(2, false),
            "selections must be sized to all 3 diff lines, or line staging silently no-ops"
        ),
        other => panic!("expected a text diff, got {other:?}"),
    }
}
