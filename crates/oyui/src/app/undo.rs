//! Bounded undo/redo stack for staging mutations.

use crate::diff::DiffResult;
use crate::diff::FileDiff;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use std::path::PathBuf;
use std::sync::Arc;

/// A point-in-time copy of staging-relevant state.
///
/// File contents are `Arc<str>` inside [`FileDiff`], so clones stay cheap; only
/// hunk layouts and line selections are copied.
pub struct StagingSnapshot {
    pub tree: FileTree,
    pub diffs: Vec<(PathBuf, FileDiff)>,
}

/// Captures the staging state of every cached text diff.
pub fn capture(tree: &FileTree, cache: &DiffCache) -> StagingSnapshot {
    let mut diffs = Vec::new();
    for f in tree.files() {
        if let Some(DiffResult::Text(d)) = cache.diffs.get(&f.path).as_deref() {
            diffs.push((f.path.clone(), d.clone()));
        }
    }
    StagingSnapshot {
        tree: tree.clone(),
        diffs,
    }
}

/// Restores a snapshot into a tree and the diff cache.
pub fn restore(tree: &mut FileTree, cache: &DiffCache, snap: StagingSnapshot) {
    tree.replace(snap.tree);
    let generation = cache.diffs.generation();
    for (path, diff) in snap.diffs {
        cache
            .diffs
            .set(path, Arc::new(DiffResult::Text(diff)), generation);
    }
}

/// Bounded undo/redo stack.
#[derive(Default)]
pub struct UndoStack {
    undo: Vec<StagingSnapshot>,
    redo: Vec<StagingSnapshot>,
}

impl UndoStack {
    /// Maximum retained history depth. Snapshots share file contents via `Arc`,
    /// so the cost is bounded by the number of cached diffs times this depth.
    pub const MAX: usize = 20;

    /// Records a snapshot for a fresh action and drops the redo branch.
    pub fn new_action(&mut self, snap: StagingSnapshot) {
        self.record(snap);
        self.redo.clear();
    }

    /// Records a snapshot without touching the redo branch.
    pub fn record(&mut self, snap: StagingSnapshot) {
        self.undo.push(snap);
        if self.undo.len() > Self::MAX {
            self.undo.remove(0);
        }
    }

    /// Pushes a snapshot onto the redo branch.
    pub fn put_redo(&mut self, snap: StagingSnapshot) {
        self.redo.push(snap);
    }

    /// Takes the most recent undo snapshot.
    pub fn take_undo(&mut self) -> Option<StagingSnapshot> {
        self.undo.pop()
    }

    /// Takes the most recent redo snapshot.
    pub fn take_redo(&mut self) -> Option<StagingSnapshot> {
        self.redo.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, Hunk, LineSelections};
    use crate::tree::StagingState;

    fn setup() -> (FileTree, DiffCache) {
        let mut tree = FileTree::default();
        tree.insert_file(
            PathBuf::from("a.txt"),
            None,
            Some(PathBuf::from("right/a.txt")),
        );
        let cache = DiffCache::default();
        let diff = FileDiff {
            old_file_content: Arc::from(""),
            new_file_content: Arc::from("x"),
            hunks: vec![Hunk {
                before_lines: 0..0,
                after_lines: 0..1,
                lines: vec![DiffLine::Addition {
                    new_line_idx: 0,
                    inline_highlights: Vec::new(),
                }],
                marker: Default::default(),
            }],
            line_selections: LineSelections::default(),
        };
        cache.diffs.set(
            PathBuf::from("a.txt"),
            Arc::new(DiffResult::Text(diff)),
            cache.diffs.generation(),
        );
        (tree, cache)
    }

    #[test]
    fn restore_reverts_tree_and_line_selections() {
        let (mut tree, cache) = setup();
        let snap = capture(&tree, &cache);

        tree.set_state_for_path(&PathBuf::from("a.txt"), StagingState::Staged);
        cache.diffs.update(&PathBuf::from("a.txt"), |d| {
            if let DiffResult::Text(fd) = d {
                fd.set_all_staging(false, true);
            }
        });

        restore(&mut tree, &cache, snap);

        assert_eq!(
            tree.get_file_state(&PathBuf::from("a.txt")),
            Some(StagingState::Unstaged)
        );
        let restored = cache.diffs.get(&PathBuf::from("a.txt")).unwrap();
        let DiffResult::Text(fd) = restored.as_ref() else {
            panic!("expected text diff");
        };
        assert_eq!(fd.staging_state(false), StagingState::Unstaged);
    }
}
