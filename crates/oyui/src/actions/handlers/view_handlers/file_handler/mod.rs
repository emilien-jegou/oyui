use std::path::Path;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::TreeNode;
use crate::worker::tasks;

pub mod file_staging_handler;

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
    path: &Path,
    target_row: usize,
    target_offset: Option<usize>,
) {
    let state = view.scroll_states.entry(path.to_path_buf()).or_default();
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

    fn page_up(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let page_size = view.last_height.saturating_sub(2);
            let target_row = (ctx.current_row_idx as isize - page_size as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn page_down(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            let page_size = view.last_height.saturating_sub(2);
            let target_row = (ctx.current_row_idx as isize + page_size as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn top(&self) {
        let mut view = self.view.file_view.write();
        if let Some(ctx) = get_file_context(&view) {
            update_scroll_state(&mut view, &ctx.path, 0, None);
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

impl ViewFileInlineDiffActionsHandler for AppActionsHandler {
    fn toggle(&self) {
        let enabled = {
            let mut flag = self.inline_diff.write();
            *flag = !*flag;
            *flag
        };
        tracing::debug!(enabled, "Toggled inline diff");

        let path = self.view.file_view.read().current_path.clone();
        if let Some(path) = path {
            self.cache.write().diffs.mark_started(path.clone());

            let tree_read = self.tree.read();
            fn find_paths(
                nodes: &[TreeNode],
                path: &std::path::Path,
            ) -> Option<(Option<std::path::PathBuf>, Option<std::path::PathBuf>)> {
                for node in nodes {
                    match node {
                        TreeNode::File(f) => {
                            if f.path == path {
                                return Some((f.left_path.clone(), f.right_path.clone()));
                            }
                        }
                        TreeNode::Directory(d) => {
                            if let Some(paths) = find_paths(&d.children, path) {
                                return Some(paths);
                            }
                        }
                    }
                }
                None
            }
            if let Some((left_path, right_path)) = find_paths(&tree_read.nodes, &path) {
                let _ = self.worker.send(tasks::full_diff::FullDiffReq {
                    node_path: path,
                    left_path,
                    right_path,
                });
            }
        }
    }

    fn set(&self, val: bool) {
        *self.inline_diff.write() = val;
        tracing::debug!(val, "Set inline diff");
    }
}
