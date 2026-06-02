use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::sync::Arc;

struct FileContext {
    path: std::path::PathBuf,
    max_idx: usize,
    current_row_idx: usize,
    cursor_screen_offset: usize,
}

fn get_file_context(view: &crate::view::file::FileViewData) -> Option<FileContext> {
    let path = view.current_path.clone()?;
    let max_idx = view
        .row_counts
        .get(&path)
        .map(|&c| c.saturating_sub(1))
        .unwrap_or(0);

    let (current_row_idx, current_offset) = {
        let s = view.scroll_states.get(&path);
        (
            s.and_then(|st| st.selected()).unwrap_or(0),
            s.map(|st| st.offset()).unwrap_or(0),
        )
    };
    let cursor_screen_offset = current_row_idx.saturating_sub(current_offset);

    Some(FileContext {
        path,
        max_idx,
        current_row_idx,
        cursor_screen_offset,
    })
}

fn update_scroll_state(
    view: &mut crate::view::file::FileViewData,
    path: &std::path::PathBuf,
    target_row: usize,
    target_offset: Option<usize>,
) {
    let state = view.scroll_states.entry(path.clone()).or_default();
    state.select(Some(target_row));
    if let Some(off) = target_offset {
        *state.offset_mut() = off;
    }
}

fn handle_hscroll(
    view: &mut crate::view::file::FileViewData,
    path: &std::path::PathBuf,
    delta: isize,
    cache: &DiffCache,
) {
    let mut max_line_len = 0;

    if let Some(crate::diff::DiffResult::Text(diff)) = cache.diffs.get(path).value() {
        let old_max = diff
            .old_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let new_max = diff
            .new_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        max_line_len = old_max.max(new_max);
    }

    let code_col_width = view.last_width.saturating_sub(6);
    let max_hscroll = max_line_len.saturating_sub(code_col_width) + 10;

    let hs = view.hscroll_states.entry(path.clone()).or_insert(0);
    *hs = (*hs as isize + delta).clamp(0, max_hscroll as isize) as usize;
}

fn toggle_stage_hunk_handler(
    tree_rw: &Arc<RwLock<FileTree>>,
    cache_rw: &Arc<RwLock<DiffCache>>,
    path: &std::path::PathBuf,
    hunk_idx: usize,
) {
    let mut diff_clone = None;
    if let Some(val) = cache_rw.read().diffs.get(path).value() {
        diff_clone = Some(val.clone());
    }

    if let Some(mut diff_result) = diff_clone {
        if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
            let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();

            let default_staged = tree_rw
                .read()
                .get_file_state(path)
                .unwrap_or(crate::tree::StagingState::Unstaged)
                == crate::tree::StagingState::Staged;

            if diff.line_selections.len() != total_lines {
                diff.line_selections.resize(total_lines, default_staged);
            }

            let mut start_idx = 0;
            for hunk in diff.hunks.iter().take(hunk_idx) {
                start_idx += hunk.lines.len();
            }

            if let Some(hunk) = diff.hunks.get(hunk_idx) {
                let mut all_staged = true;
                for (j, line) in hunk.lines.iter().enumerate() {
                    if matches!(
                        line,
                        crate::diff::DiffLine::Addition { .. }
                            | crate::diff::DiffLine::Deletion { .. }
                    ) && !diff
                        .line_selections
                        .get(start_idx + j)
                        .copied()
                        .unwrap_or(default_staged)
                    {
                        all_staged = false;
                        break;
                    }
                }

                let new_state = !all_staged;
                for (j, line) in hunk.lines.iter().enumerate() {
                    if matches!(
                        line,
                        crate::diff::DiffLine::Addition { .. }
                            | crate::diff::DiffLine::Deletion { .. }
                    ) && start_idx + j < diff.line_selections.len()
                    {
                        diff.line_selections[start_idx + j] = new_state;
                    }
                }
            }

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
                            .unwrap_or(default_staged);
                        if is_staged {
                            has_staged = true;
                        } else {
                            has_unstaged = true;
                        }
                    }
                    current_idx += 1;
                }
            }

            let new_staging_state = if has_staged && has_unstaged {
                crate::tree::StagingState::PartiallyStaged
            } else if has_staged {
                crate::tree::StagingState::Staged
            } else {
                crate::tree::StagingState::Unstaged
            };

            // Update file state in tree
            fn find_and_update(
                nodes: &mut [crate::tree::TreeNode],
                path: &std::path::PathBuf,
                new_state: crate::tree::StagingState,
            ) -> bool {
                for node in nodes {
                    match node {
                        crate::tree::TreeNode::File(f) => {
                            if f.path == *path {
                                f.state = new_state;
                                return true;
                            }
                        }
                        crate::tree::TreeNode::Directory(d) => {
                            if find_and_update(&mut d.children, path, new_state) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            let mut tree = tree_rw.write();
            find_and_update(&mut tree.nodes, path, new_staging_state);
        }

        cache_rw.write().diffs.set(path.clone(), diff_result);
    }
}

fn get_target_stage_action(
    view: &crate::view::file::FileViewData,
    path: &std::path::PathBuf,
    current_selected: usize,
) -> Option<usize> {
    let mut target_hunk = None;
    if let Some(mapping) = view.row_to_hunk.get(path) {
        if let Some(&Some(hunk_idx)) = mapping.get(current_selected) {
            target_hunk = Some(hunk_idx);
        } else {
            let mut closest_dist = usize::MAX;
            for (idx, &hunk_opt) in mapping.iter().enumerate() {
                if let Some(h) = hunk_opt {
                    let dist = current_selected.abs_diff(idx);
                    if dist < closest_dist {
                        closest_dist = dist;
                        target_hunk = Some(h);
                    }
                }
            }
            if closest_dist > 4 {
                target_hunk = None;
            }
        }
    }
    target_hunk
}

impl ViewFileActionsHandler for AppActionsHandler {
    fn close(&self) {
        *self.view.current.write() = crate::view::ViewKind::Tree;
        self.view.file_view.write().current_path = None;
    }
}

impl ViewFileScrollActionsHandler for AppActionsHandler {
    fn left(&self, val: u32) {
        let mut view = self.view.file_view.write();
        let cache = self.cache.read();
        if let Some(ctx) = get_file_context(&view) {
            handle_hscroll(&mut view, &ctx.path, -(val as isize * 4), &cache);
        }
    }

    fn right(&self, val: u32) {
        let mut view = self.view.file_view.write();
        let cache = self.cache.read();
        if let Some(ctx) = get_file_context(&view) {
            handle_hscroll(&mut view, &ctx.path, val as isize * 4, &cache);
        }
    }
}

impl ViewFileCursorActionsHandler for AppActionsHandler {
    fn up(&self, val: u32) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let target_row = (ctx.current_row_idx as isize - val as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn down(&self, val: u32) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let target_row = (ctx.current_row_idx as isize + val as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn half_page_up(&self) {
        ViewFileCursorActionsHandler::up(self, 20);
    }

    fn half_page_down(&self) {
        ViewFileCursorActionsHandler::down(self, 20);
    }

    fn top(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            if view.pending_g {
                update_scroll_state(&mut view, &ctx.path, 0, None);
                view.pending_g = false;
            } else {
                view.pending_g = true;
            }
        }
    }

    fn bottom(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            update_scroll_state(&mut view, &ctx.path, ctx.max_idx, None);
        }
    }
}

impl ViewFileNavActionsHandler for AppActionsHandler {
    fn next_hunk(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let last_height = view.last_height;
            if let Some(starts) = view.hunk_starts.get(&ctx.path) {
                let target = starts
                    .iter()
                    .find(|&&idx| idx > ctx.current_row_idx)
                    .or_else(|| starts.first());

                if let Some(&t) = target {
                    let padding = last_height.saturating_sub(1) / 3;
                    let target_offset = Some(t.saturating_sub(padding));
                    update_scroll_state(&mut view, &ctx.path, t, target_offset);
                }
            }
        }
    }

    fn prev_hunk(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let last_height = view.last_height;
            if let Some(starts) = view.hunk_starts.get(&ctx.path) {
                let target = starts
                    .iter()
                    .rev()
                    .find(|&&idx| idx < ctx.current_row_idx)
                    .or_else(|| starts.last());

                if let Some(&t) = target {
                    let padding = last_height.saturating_sub(1) / 3;
                    let target_offset = Some(t.saturating_sub(padding));
                    update_scroll_state(&mut view, &ctx.path, t, target_offset);
                }
            }
        }
    }
}

impl ViewFileStagingActionsHandler for AppActionsHandler {
    fn toggle(&self) {
        let view = self.view.file_view.read();
        if let Some(ctx) = get_file_context(&view) {
            drop(view);
            let view_read = self.view.file_view.read();

            let hidx = get_target_stage_action(&view_read, &ctx.path, ctx.current_row_idx);
            drop(view_read);
            if let Some(hunk_idx) = hidx {
                toggle_stage_hunk_handler(&self.tree, &self.cache, &ctx.path, hunk_idx);
            }
        }
    }

    fn toggle_hunk(&self, val: u32) {
        let view = self.view.file_view.read();
        if let Some(ctx) = get_file_context(&view) {
            drop(view);
            toggle_stage_hunk_handler(&self.tree, &self.cache, &ctx.path, val as usize);
        }
    }
}

impl ViewFileFoldActionsHandler for AppActionsHandler {
    fn toggle(&self) {
        let mut view = self.view.file_view.write();
        let cache = self.cache.read();
        if let Some(ctx) = get_file_context(&view) {
            let mut target_logical = 0;
            if let Some(mapping) = view.line_mapping.get(&ctx.path) {
                target_logical = mapping.get(ctx.current_row_idx).copied().unwrap_or(0);
            }

            view.is_folded = !view.is_folded;

            let next_selected = if let Some(crate::diff::DiffResult::Text(diff)) =
                cache.diffs.get(&ctx.path).value()
            {
                let new_lines_len = diff.new_text.lines().count();
                let new_map = view.get_line_map(diff, new_lines_len);

                new_map
                    .iter()
                    .position(|&l| l >= target_logical)
                    .unwrap_or(new_map.len().saturating_sub(1))
            } else {
                0
            };

            let next_offset = Some(next_selected.saturating_sub(ctx.cursor_screen_offset));
            update_scroll_state(&mut view, &ctx.path, next_selected, next_offset);
        }
    }
}
