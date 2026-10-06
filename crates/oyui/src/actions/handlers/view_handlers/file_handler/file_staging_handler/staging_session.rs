//! Staging session: copy-on-write diff mutation for the open file.

use crate::app::UiState;
use crate::diff::FileDiff;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;

pub struct StagingSession {
    pub path: PathBuf,
    pub current_row_idx: usize,
    pub hunk_idx: Option<usize>,
    pub hunk_visual_start: Option<usize>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub ui: Arc<Mutex<UiState>>,
    pub operation: crate::app::Operation,
}

impl StagingSession {
    /// Builds a session for the open file, refreshing its layout first.
    pub fn try_new(
        tree: Arc<RwLock<FileTree>>,
        cache: DiffCache,
        ui: Arc<Mutex<UiState>>,
        operation: crate::app::Operation,
    ) -> Option<Self> {
        let mut guard = ui.lock();

        // Diff mutations (split/join) can happen between draws — notably
        // inside one batched key burst — refresh before trusting the layout.
        let path = guard.file_view.current_path.clone()?;
        let regions = guard
            .resolve
            .as_ref()
            .map(|r| r.regions())
            .unwrap_or_default();
        guard.file_view.conflict_regions = regions;
        if let Some(crate::diff::DiffResult::Text(diff)) = cache.diffs.get(&path).as_deref() {
            guard.file_view.recompute_view_model(diff);
        }

        let current_row_idx = guard
            .file_view
            .scroll_states
            .get(&path)
            .and_then(|st| st.selected())
            .unwrap_or(0);

        let mut hunk_idx = None;
        let mut hunk_visual_start = None;

        if let Some(mappings) = guard.file_view.row_to_hunk(&path) {
            if let Some(Some(h_idx)) = mappings.get(current_row_idx) {
                hunk_idx = Some(*h_idx);
                hunk_visual_start = mappings.iter().position(|&h| h == Some(*h_idx));
            }
        }

        drop(guard);

        Some(Self {
            path,
            current_row_idx,
            hunk_idx,
            hunk_visual_start,
            tree,
            cache,
            ui,
            operation,
        })
    }

    /// True in merge sessions, where staging selections never take effect.
    pub fn is_merge(&self) -> bool {
        self.operation == crate::app::Operation::Merge
    }

    /// Mutates the open file's diff in place; callers persist via `CacheMap::update`.
    pub fn mutate_diff<F>(&self, f: F)
    where
        F: FnOnce(&mut FileDiff, &RwLock<FileTree>),
    {
        // The closure may take the tree lock (staging state updates); see
        // `CacheMap::update` for the lock-ordering invariant this relies on.
        self.cache.diffs.update(&self.path, |diff_result| {
            if let crate::diff::DiffResult::Text(diff) = diff_result {
                f(diff, &self.tree);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, DiffResult, FileDiff, Hunk, HunkMarker, LineSelections};
    use crate::view::file::view_model::FileViewModel;
    use ratatui::widgets::TableState;
    use std::path::PathBuf;

    fn two_hunk_diff() -> FileDiff {
        let new_content = (0..40)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let old_content = (0..38)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let hunk = |s: usize| Hunk {
            before_lines: s..s + 3,
            after_lines: s..s + 5,
            lines: vec![
                DiffLine::Context {
                    new_line_idx: s,
                    old_line_idx: s,
                },
                DiffLine::Addition {
                    new_line_idx: s + 1,
                    inline_highlights: Vec::new(),
                },
                DiffLine::Context {
                    new_line_idx: s + 2,
                    old_line_idx: s + 1,
                },
                DiffLine::Addition {
                    new_line_idx: s + 3,
                    inline_highlights: Vec::new(),
                },
                DiffLine::Context {
                    new_line_idx: s + 4,
                    old_line_idx: s + 2,
                },
            ],
            marker: Default::default(),
        };
        FileDiff {
            old_file_content: old_content.into(),
            new_file_content: new_content.into(),
            hunks: vec![hunk(0), hunk(30)],
            line_selections: LineSelections::default(),
        }
    }

    /// A split mutates the cached diff in place between draws; the session
    /// must map the cursor through the refreshed layout, not the stale one
    /// (regression: space/s acted on the wrong hunk after a split).
    #[test]
    fn session_maps_cursor_through_layout_after_in_place_split() {
        let path = PathBuf::from("a.txt");
        let cache = DiffCache::default();
        let diff = two_hunk_diff();
        cache.diffs.set(
            path.clone(),
            Arc::new(DiffResult::Text(diff)),
            cache.diffs.generation(),
        );

        let ui = Arc::new(Mutex::new(UiState::new(false)));
        let folded = ui.lock().file_view.is_folded;

        // First draw computes the model; cursor sits inside hunk 0 (row 2).
        {
            let mut guard = ui.lock();
            guard.file_view.current_path = Some(path.clone());
            let cached = cache.diffs.get(&path).unwrap();
            let DiffResult::Text(diff) = (*cached).clone() else {
                unreachable!("fixture is a text diff")
            };
            guard.file_view.recompute_view_model(&diff);
            guard
                .file_view
                .scroll_states
                .entry(path.clone())
                .or_insert_with(TableState::default)
                .select(Some(2));
        }

        // Split hunk 0 in place — exactly what the 's' key does.
        cache.diffs.update(&path, |d| {
            if let DiffResult::Text(t) = d {
                t.split_hunk(0, 2, HunkMarker::HunkSplit);
            }
        });

        let tree = Arc::new(RwLock::new(FileTree::default()));
        let session = StagingSession::try_new(
            tree,
            cache.clone(),
            ui,
            crate::app::Operation::Diff,
        )
        .expect("session for the open file");
        assert_eq!(session.current_row_idx, 2);

        // Reference: what the mapping says with a freshly recomputed model.
        let cached = cache.diffs.get(&path).unwrap();
        let DiffResult::Text(mutated) = (*cached).clone() else {
            unreachable!("fixture is a text diff")
        };
        let mut fresh = FileViewModel::default();
        let fresh_path = path.clone();
        fresh.recompute(
            &fresh_path,
            &mutated,
            folded,
            4,
            &crate::view::file::view_model::ConflictRegions::default(),
        );
        let expected =
            fresh.row_to_hunk(&fresh_path).expect("fresh mapping")[session.current_row_idx];

        assert_eq!(
            session.hunk_idx, expected,
            "cursor must map through the post-split layout"
        );
        // ...and through the stale layout it would pick the original hunk 0.
        assert_ne!(session.hunk_idx, Some(0), "stale layout would say hunk 0");
        assert_eq!(
            session.hunk_visual_start,
            fresh
                .row_to_hunk(&fresh_path)
                .expect("fresh mapping")
                .iter()
                .position(|h| *h == expected),
            "visual start must match the post-split layout"
        );
    }
}
