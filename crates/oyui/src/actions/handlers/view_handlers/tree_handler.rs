//! Tree view actions plus staging synchronization for the file tree.
use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crate::worker::events::file_opened::FileOpened;
use tracing::debug;

impl ViewTreeActionsHandler for AppActionsHandler {
    fn open_selected(&self) {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();

        if let Some(row) = ui.tree_view.selected_row(&tree, &self.cache) {
            if row.is_dir {
                ui.tree_view.ui_state.set_folded(&row.path, false);
            } else {
                debug!("Opened selected");
                ui.current = crate::view::ViewKind::File;
                ui.file_view.current_path = Some(row.path.clone());
                let _ = self.worker.send(FileOpened { path: row.path });
            }
        }
    }

    fn open_file(&self, file: String) {
        let mut ui = self.ui.lock();
        ui.current = crate::view::ViewKind::File;
        let path = std::path::PathBuf::from(file);
        ui.file_view.current_path = Some(path.clone());
        debug!("Opened file");
        let _ = self.worker.send(FileOpened { path });
    }

    fn selected_index(&self) -> u32 {
        self.ui.lock().tree_view.selected_index as u32
    }

    fn row_count(&self) -> u32 {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        ui.tree_view.flat_rows(&tree, &self.cache).len() as u32
    }

    fn selected_path(&self) -> String {
        self.selected_row()
            .map(|r| r.path.display().to_string())
            .unwrap_or_default()
    }

    fn selected_name(&self) -> String {
        self.selected_row().map(|r| r.name).unwrap_or_default()
    }

    fn selected_is_dir(&self) -> bool {
        self.selected_row().map(|r| r.is_dir).unwrap_or(false)
    }

    fn selected_staging(&self) -> String {
        match self.selected_row().map(|r| r.staging_state) {
            Some(crate::tree::StagingState::Staged) => "staged".into(),
            Some(crate::tree::StagingState::PartiallyStaged) => "partial".into(),
            Some(crate::tree::StagingState::Unstaged) => "unstaged".into(),
            None => "none".into(),
        }
    }

    fn is_folded(&self, path: String) -> bool {
        self.ui
            .lock()
            .tree_view
            .ui_state
            .is_folded(&std::path::PathBuf::from(path))
    }
}

impl AppActionsHandler {
    /// Clones the selected tree row, if any.
    fn selected_row(&self) -> Option<crate::view::tree::TreeRow> {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        ui.tree_view.selected_row(&tree, &self.cache)
    }
}

impl ViewTreeCursorActionsHandler for AppActionsHandler {
    fn up(&self, val: u32) {
        let view = &mut self.ui.lock().tree_view;
        view.selected_index = view.selected_index.saturating_sub(val as usize);
    }

    fn down(&self, val: u32) {
        let tree = self.tree.read();
        let view = &mut self.ui.lock().tree_view;

        let len = view.flat_rows(&tree, &self.cache).len();
        let max_idx = len.saturating_sub(1);
        view.selected_index = (view.selected_index + val as usize).min(max_idx);
    }

    fn half_page_up(&self) {
        ViewTreeCursorActionsHandler::up(self, 20);
    }

    fn half_page_down(&self) {
        ViewTreeCursorActionsHandler::down(self, 20);
    }

    fn page_up(&self) {
        let view = &mut self.ui.lock().tree_view;
        let page_size = view.last_height.saturating_sub(2).max(1);
        view.selected_index = view.selected_index.saturating_sub(page_size);
    }

    fn page_down(&self) {
        let tree = self.tree.read();
        let view = &mut self.ui.lock().tree_view;
        let len = view.flat_rows(&tree, &self.cache).len();
        let max_idx = len.saturating_sub(1);
        let page_size = view.last_height.saturating_sub(2).max(1);
        view.selected_index = (view.selected_index + page_size).min(max_idx);
    }

    fn top(&self) {
        let view = &mut self.ui.lock().tree_view;
        view.selected_index = 0;
    }

    fn bottom(&self) {
        let tree = self.tree.read();
        let view = &mut self.ui.lock().tree_view;

        let len = view.flat_rows(&tree, &self.cache).len();
        let max_idx = len.saturating_sub(1);
        view.selected_index = max_idx;
    }
}

impl ViewTreeDirectoryActionsHandler for AppActionsHandler {
    fn expand(&self) {
        let tree = self.tree.read();
        let view = &mut self.ui.lock().tree_view;

        if let Some(row) = view.selected_row(&tree, &self.cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, false);
                view.mark_dirty();
            }
        }
    }

    fn collapse(&self) {
        let tree = self.tree.read();
        let view = &mut self.ui.lock().tree_view;

        if let Some(row) = view.selected_row(&tree, &self.cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, true);
                view.mark_dirty();
            }
        }
    }

    fn expand_all(&self) {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        visit_dirs(&tree.nodes, &mut |path| {
            ui.tree_view.ui_state.set_folded(path, false)
        });
        ui.tree_view.mark_dirty();
    }

    fn collapse_all(&self) {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        visit_dirs(&tree.nodes, &mut |path| {
            ui.tree_view.ui_state.set_folded(path, true)
        });
        ui.tree_view.mark_dirty();
    }
}

/// Visits every directory path in the tree, depth-first.
fn visit_dirs(nodes: &[crate::tree::TreeNode], f: &mut impl FnMut(&std::path::Path)) {
    for node in nodes {
        if let crate::tree::TreeNode::Directory(dir) = node {
            f(&dir.path);
            visit_dirs(&dir.children, f);
        }
    }
}

impl ViewTreeStagingActionsHandler for AppActionsHandler {
    fn toggle_selected(&self) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        let tree_guard = self.tree.read();
        let picked = {
            let mut ui = self.ui.lock();
            ui.tree_view
                .selected_row(&tree_guard, &self.cache)
                .map(|row| (row.staging_state.toggle(), row.path))
        };
        drop(tree_guard);

        let Some((new_state, path_clone)) = picked else {
            return;
        };

        tracing::debug!(path = %path_clone.display(), ?new_state, "Toggling stage state");
        let mut tree_write = self.tree.write();
        crate::app::commands::set_state_for_path(&mut tree_write, &path_clone, new_state);

        sync_cache(&tree_write, &self.cache);
    }

    fn invert(&self) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        tracing::debug!("Inverting all staging selections");
        let mut tree_write = self.tree.write();

        for f in tree_write.files_mut() {
            let updated = self.cache.diffs.update(&f.path, |diff_result| {
                if let crate::diff::DiffResult::Text(diff) = diff_result {
                    let default_staged = f.state == crate::tree::StagingState::Staged;
                    diff.invert_staging(default_staged);
                    f.state = diff.staging_state(!default_staged);
                    true
                } else {
                    false
                }
            });

            // No cached diff (or a non-text one): fall back to a plain toggle.
            if updated.is_none() {
                f.state = f.state.toggle();
            }
        }
    }

    fn set(&self, path: String, staged: bool) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        let state = if staged {
            crate::tree::StagingState::Staged
        } else {
            crate::tree::StagingState::Unstaged
        };
        let mut tree = self.tree.write();
        tree.set_state_for_path(&std::path::PathBuf::from(path), state);
        sync_cache(&tree, &self.cache);
    }

    fn set_matching(&self, pattern: String, staged: bool) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        let state = if staged {
            crate::tree::StagingState::Staged
        } else {
            crate::tree::StagingState::Unstaged
        };
        let mut tree = self.tree.write();
        let paths: Vec<std::path::PathBuf> = tree
            .files()
            .filter(|f| crate::commons::glob::glob_match(&pattern, &f.path))
            .map(|f| f.path.clone())
            .collect();
        for path in paths {
            tree.set_state_for_path(&path, state);
        }
        sync_cache(&tree, &self.cache);
    }

    fn stage_all(&self) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        let mut tree = self.tree.write();
        for f in tree.files_mut() {
            f.state = crate::tree::StagingState::Staged;
        }
        sync_cache(&tree, &self.cache);
    }

    fn unstage_all(&self) {
        if self.reject_read_only() || self.reject_merge() {
            return;
        }
        self.push_undo_snapshot();
        let mut tree = self.tree.write();
        for f in tree.files_mut() {
            f.state = crate::tree::StagingState::Unstaged;
        }
        sync_cache(&tree, &self.cache);
    }
}

pub(crate) fn sync_cache(tree: &FileTree, cache: &DiffCache) {
    for f in tree.files() {
        if f.state == crate::tree::StagingState::Staged
            || f.state == crate::tree::StagingState::Unstaged
        {
            let target_val = f.state == crate::tree::StagingState::Staged;

            cache.diffs.update(&f.path, |diff_result| {
                if let crate::diff::DiffResult::Text(diff) = diff_result {
                    let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
                    diff.line_selections.ensure_size(total_lines, target_val);
                }
            });
        }
    }
}
