use crate::diff::FileDiff;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub struct StagingSession {
    pub path: PathBuf,
    pub current_row_idx: usize,
    pub hunk_idx: Option<usize>,
    pub hunk_visual_start: Option<usize>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: Arc<RwLock<DiffCache>>,
    pub view: Arc<RwLock<crate::view::file::FileViewData>>,
}

impl StagingSession {
    pub fn try_new(
        tree: Arc<RwLock<FileTree>>,
        cache: Arc<RwLock<DiffCache>>,
        view: Arc<RwLock<crate::view::file::FileViewData>>,
    ) -> Option<Self> {
        let view_guard = view.read();
        let path = view_guard.current_path.clone()?;

        let current_row_idx = {
            let s = view_guard.scroll_states.get(&path);
            s.and_then(|st| st.selected()).unwrap_or(0)
        };

        let mut hunk_idx = None;
        let mut hunk_visual_start = None;

        if let Some(mappings) = view_guard.row_to_hunk.get(&path) {
            if let Some(Some(h_idx)) = mappings.get(current_row_idx) {
                hunk_idx = Some(*h_idx);
                hunk_visual_start = mappings.iter().position(|&h| h == Some(*h_idx));
            }
        }

        drop(view_guard);

        Some(Self {
            path,
            current_row_idx,
            hunk_idx,
            hunk_visual_start,
            tree,
            cache,
            view,
        })
    }

    pub fn mutate_diff<F>(&self, f: F)
    where
        F: FnOnce(&mut FileDiff, &RwLock<FileTree>),
    {
        let mut cache_guard = self.cache.write();
        if let Some(mut diff_result) = cache_guard.diffs.get(&self.path).value().cloned()
        {
            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                f(diff, &self.tree);
            }
            cache_guard.diffs.set(self.path.clone(), diff_result);
        }
    }
}
