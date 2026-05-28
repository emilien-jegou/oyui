use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::tree::FileTree;

impl ViewTreeActionsHandler for AppActionsHandler {
    fn open_selected(&self) {
        let tree = self.tree.read();
        let cache = self.cache.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, false);
            } else {
                *self.view.current.write() = crate::view::ViewKind::File;
                self.view.file_view.write().current_path = Some(row.path.clone());
            }
        }
    }

    fn open_file(&self, file: String) {
        *self.view.current.write() = crate::view::ViewKind::File;
        self.view.file_view.write().current_path = Some(std::path::PathBuf::from(file));
    }
}

impl ViewTreeCursorActionsHandler for AppActionsHandler {
    fn up(&self, val: u32) {
        let mut view = self.view.tree_view.write();
        view.selected_index = view.selected_index.saturating_sub(val as usize);
    }

    fn down(&self, val: u32) {
        let tree = self.tree.read();
        let cache = self.cache.read();
        let mut view = self.view.tree_view.write();

        let len = view.flat_rows(&tree, &cache).len();
        let max_idx = len.saturating_sub(1);
        view.selected_index = (view.selected_index + val as usize).min(max_idx);
    }

    fn half_page_up(&self) {
        ViewTreeCursorActionsHandler::up(self, 20);
    }

    fn half_page_down(&self) {
        ViewTreeCursorActionsHandler::down(self, 20);
    }

    fn top(&self) {
        let mut view = self.view.tree_view.write();
        view.selected_index = 0;
    }

    fn bottom(&self) {
        let tree = self.tree.read();
        let cache = self.cache.read();
        let mut view = self.view.tree_view.write();

        let len = view.flat_rows(&tree, &cache).len();
        let max_idx = len.saturating_sub(1);
        view.selected_index = max_idx;
    }
}

impl ViewTreeDirectoryActionsHandler for AppActionsHandler {
    fn expand(&self) {
        let tree = self.tree.read();
        let cache = self.cache.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, false);
            }
        }
    }

    fn collapse(&self) {
        let tree = self.tree.read();
        let cache = self.cache.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, true);
            }
        }
    }
}

impl ViewTreeStagingActionsHandler for AppActionsHandler {
    fn toggle_selected(&self) {
        let tree_guard = self.tree.read();
        let cache_guard = self.cache.read();
        let view = self.view.tree_view.read();

        if let Some(row) = view.selected_row(&tree_guard, &cache_guard) {
            let new_state = row.staging_state.toggle();
            let path_clone = row.path.clone();
            drop(tree_guard);
            drop(cache_guard);
            drop(view);

            tracing::debug!(path = %path_clone.display(), ?new_state, "Toggling stage state");
            let mut tree_write = self.tree.write();
            crate::app::commands::set_state_for_path(&mut tree_write, &path_clone, new_state);

            let mut cache_write = self.cache.write();
            sync_cache(&tree_write, &mut cache_write);
        }
    }

    fn invert(&self) {
        tracing::debug!("Inverting all staging selections");
        let mut tree_write = self.tree.write();
        for node in &mut tree_write.nodes {
            node.invert_state_recursive();
        }

        let mut cache_write = self.cache.write();
        sync_cache(&tree_write, &mut cache_write);
    }
}

pub(crate) fn sync_cache(tree: &FileTree, cache: &mut crate::diff_cache::DiffCache) {
    fn sync_cache_recursive(
        nodes: &[crate::tree::TreeNode],
        cache: &mut crate::diff_cache::DiffCache,
    ) {
        for node in nodes {
            match node {
                crate::tree::TreeNode::File(f) => {
                    if f.state == crate::tree::StagingState::Staged
                        || f.state == crate::tree::StagingState::Unstaged
                    {
                        let target_val = f.state == crate::tree::StagingState::Staged;

                        let mut diff_clone = None;
                        if let Some(val) = cache.diffs.get(&f.path).value() {
                            if let crate::diff::DiffResult::Text(diff) = val {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                let needs_sync = diff.line_selections.len() != total_lines
                                    || diff.line_selections.iter().any(|&v| v != target_val);

                                if needs_sync {
                                    diff_clone = Some(val.clone());
                                }
                            }
                        }

                        if let Some(mut diff_result) = diff_clone {
                            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                diff.line_selections.clear();
                                diff.line_selections.resize(total_lines, target_val);
                            }
                            cache.diffs.set(f.path.clone(), diff_result);
                        }
                    }
                }
                crate::tree::TreeNode::Directory(d) => {
                    sync_cache_recursive(&d.children, cache);
                }
            }
        }
    }
    sync_cache_recursive(&tree.nodes, cache);
}
