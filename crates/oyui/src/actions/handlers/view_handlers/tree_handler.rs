use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;

impl ViewTreeActionsHandler for AppActionsHandler {
    fn open_selected(&self) {
        let tree = self.tree.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &self.cache) {
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
        let mut view = self.view.tree_view.write();

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
        let mut view = self.view.tree_view.write();
        let page_size = view.last_height.saturating_sub(2).max(1);
        view.selected_index = view.selected_index.saturating_sub(page_size);
    }

    fn page_down(&self) {
        let tree = self.tree.read();
        let mut view = self.view.tree_view.write();
        let len = view.flat_rows(&tree, &self.cache).len();
        let max_idx = len.saturating_sub(1);
        let page_size = view.last_height.saturating_sub(2).max(1);
        view.selected_index = (view.selected_index + page_size).min(max_idx);
    }

    fn top(&self) {
        let mut view = self.view.tree_view.write();
        view.selected_index = 0;
    }

    fn bottom(&self) {
        let tree = self.tree.read();
        let mut view = self.view.tree_view.write();

        let len = view.flat_rows(&tree, &self.cache).len();
        let max_idx = len.saturating_sub(1);
        view.selected_index = max_idx;
    }
}

impl ViewTreeDirectoryActionsHandler for AppActionsHandler {
    fn expand(&self) {
        let tree = self.tree.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &self.cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, false);
            }
        }
    }

    fn collapse(&self) {
        let tree = self.tree.read();
        let mut view = self.view.tree_view.write();

        if let Some(row) = view.selected_row(&tree, &self.cache) {
            if row.is_dir {
                view.ui_state.set_folded(&row.path, true);
            }
        }
    }
}

impl ViewTreeStagingActionsHandler for AppActionsHandler {
    fn toggle_selected(&self) {
        let tree_guard = self.tree.read();
        let view = self.view.tree_view.read();

        if let Some(row) = view.selected_row(&tree_guard, &self.cache) {
            let new_state = row.staging_state.toggle();
            let path_clone = row.path.clone();
            drop(tree_guard);
            drop(view);

            tracing::debug!(path = %path_clone.display(), ?new_state, "Toggling stage state");
            let mut tree_write = self.tree.write();
            crate::app::commands::set_state_for_path(&mut tree_write, &path_clone, new_state);

            sync_cache(&tree_write, &self.cache);
        }
    }

    fn invert(&self) {
        tracing::debug!("Inverting all staging selections");
        let mut tree_write = self.tree.write();

        fn invert_recursive(
            nodes: &mut [crate::tree::TreeNode],
            cache: &DiffCache,
        ) {
            for node in nodes {
                match node {
                    crate::tree::TreeNode::File(f) => {
                        let mut diff_clone = None;
                        if let Some(val) = cache.diffs.get(&f.path).value() {
                            diff_clone = Some(val.clone());
                        }

                        if let Some(mut diff_result) = diff_clone {
                            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                let default_staged = f.state == crate::tree::StagingState::Staged;

                                // Sync boolean array length first
                                if diff.line_selections.len() != total_lines {
                                    diff.line_selections.resize(total_lines, default_staged);
                                }

                                // Invert every item individually, preserving sub-hunk boundaries
                                for b in &mut diff.line_selections {
                                    *b = !*b;
                                }

                                // Evaluate new StagingState based on inverted line_selections
                                let mut has_staged = false;
                                let mut has_unstaged = false;
                                let mut current_idx = 0;
                                for h in &diff.hunks {
                                    for line in &h.lines {
                                        if matches!(
                                            line,
                                            crate::diff::DiffLine::Addition { .. }
                                                | crate::diff::DiffLine::Deletion { .. }
                                        ) {
                                            let is_staged = diff
                                                .line_selections
                                                .get(current_idx)
                                                .copied()
                                                .unwrap_or(!default_staged);
                                            if is_staged {
                                                has_staged = true;
                                            } else {
                                                has_unstaged = true;
                                            }
                                        }
                                        current_idx += 1;
                                    }
                                }

                                f.state = if has_staged && has_unstaged {
                                    crate::tree::StagingState::PartiallyStaged
                                } else if has_staged {
                                    crate::tree::StagingState::Staged
                                } else {
                                    crate::tree::StagingState::Unstaged
                                };
                            } else {
                                // For non-text diffs, typical fallback to simple toggle
                                f.state = f.state.toggle();
                            }
                            cache.diffs.set(f.path.clone(), diff_result);
                        } else {
                            // If diff isn't cached yet, fallback to a simple toggle
                            f.state = f.state.toggle();
                        }
                    }
                    crate::tree::TreeNode::Directory(d) => {
                        invert_recursive(&mut d.children, cache);
                    }
                }
            }
        }

        invert_recursive(&mut tree_write.nodes, &self.cache);
    }
}

pub(crate) fn sync_cache(tree: &FileTree, cache: &DiffCache) {
    fn sync_cache_recursive(nodes: &[crate::tree::TreeNode], cache: &DiffCache) {
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
