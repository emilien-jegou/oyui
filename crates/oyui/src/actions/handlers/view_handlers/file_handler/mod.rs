//! File view navigation, scrolling, and fold actions on the app handler.
use std::path::Path;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff::{DiffLine, DiffResult};
use crate::diff_cache::DiffCache;
use crate::tree::StagingState;

pub mod file_conflict;
pub mod file_inspect;
pub mod file_staging_handler;

/// Read-only snapshot of what the file-view cursor currently points at.
pub(crate) struct CursorInfo {
    pub row: usize,
    pub row_count: usize,
    pub hunk_index: Option<usize>,
    pub hunk_count: usize,
    pub kind: &'static str,
    pub text: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub is_staged: Option<bool>,
}

/// Resolves the cursor context for the open file, if any.
pub(crate) fn cursor_info(
    ui: &mut crate::view::file::FileViewData,
    tree: &crate::tree::FileTree,
    cache: &DiffCache,
) -> Option<CursorInfo> {
    let path = ui.current_path.clone()?;
    let row = ui
        .scroll_states
        .get(&path)
        .and_then(|s| s.selected())
        .unwrap_or(0);
    let row_count = ui.row_count(&path);
    let hunk_starts = ui.hunk_starts(&path).cloned().unwrap_or_default();
    let hunk_count = hunk_starts.len();
    let hunk_index = ui
        .row_to_hunk(&path)
        .and_then(|m| m.get(row).copied().flatten());

    let mut info = CursorInfo {
        row,
        row_count,
        hunk_index,
        hunk_count,
        kind: "none",
        text: String::new(),
        old_line: None,
        new_line: None,
        is_staged: None,
    };

    let diff_arc = cache.diffs.get(&path);
    let Some(DiffResult::Text(diff)) = diff_arc.as_deref() else {
        return Some(info);
    };

    let Some(hi) = hunk_index else {
        return Some(info);
    };
    let Some(hunk) = diff.hunks.get(hi) else {
        return Some(info);
    };
    let Some(&visual_start) = hunk_starts.get(hi) else {
        return Some(info);
    };
    let line_within = row.saturating_sub(visual_start);
    let Some(line) = hunk.lines.get(line_within) else {
        return Some(info);
    };

    let default_staged = tree.get_file_state(&path) == Some(StagingState::Staged);
    let new_lines: Vec<&str> = diff.new_file_content.lines().collect();
    let old_lines: Vec<&str> = diff.old_file_content.lines().collect();

    match line {
        DiffLine::Context {
            old_line_idx,
            new_line_idx,
        } => {
            info.kind = "context";
            info.text = new_lines
                .get(*new_line_idx)
                .copied()
                .unwrap_or("")
                .to_string();
            info.old_line = Some(*old_line_idx);
            info.new_line = Some(*new_line_idx);
        }
        DiffLine::Deletion { old_line_idx, .. } => {
            info.kind = "deletion";
            info.text = old_lines
                .get(*old_line_idx)
                .copied()
                .unwrap_or("")
                .to_string();
            info.old_line = Some(*old_line_idx);
        }
        DiffLine::Addition { new_line_idx, .. } => {
            info.kind = "addition";
            info.text = new_lines
                .get(*new_line_idx)
                .copied()
                .unwrap_or("")
                .to_string();
            info.new_line = Some(*new_line_idx);
        }
    }

    let sel_idx = diff.hunk_start_idx(hi) + line_within;
    info.is_staged = Some(diff.line_selections.get(sel_idx, default_staged));
    Some(info)
}

struct FileContext {
    path: std::path::PathBuf,
    max_idx: usize,
    current_row_idx: usize,
    cursor_screen_offset: usize,
}

fn get_file_context(view: &mut crate::view::file::FileViewData) -> Option<FileContext> {
    let path = view.current_path.clone()?;
    let max_idx = view.row_count(&path).saturating_sub(1);

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

    if let Some(DiffResult::Text(diff)) = cache.diffs.get(path).as_deref() {
        let old_max = diff
            .old_file_content
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let new_max = diff
            .new_file_content
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
        let mut ui = self.ui.lock();
        ui.current = crate::view::ViewKind::Tree;
        ui.file_view.current_path = None;
    }

    fn path(&self) -> String {
        self.ui
            .lock()
            .file_view
            .current_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    fn folded(&self) -> bool {
        self.ui.lock().file_view.is_folded
    }
}

impl ViewFileScrollActionsHandler for AppActionsHandler {
    fn left(&self, val: u32) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            handle_hscroll(&mut view, &ctx.path, -(val as isize * 4), &self.cache);
        }
    }

    fn right(&self, val: u32) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            handle_hscroll(&mut view, &ctx.path, val as isize * 4, &self.cache);
        }
    }
}

impl ViewFileCursorActionsHandler for AppActionsHandler {
    fn up(&self, val: u32) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let target_row = (ctx.current_row_idx as isize - val as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn down(&self, val: u32) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
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
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let page_size = view.last_height.saturating_sub(2);
            let target_row = (ctx.current_row_idx as isize - page_size as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn page_down(&self) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let page_size = view.last_height.saturating_sub(2);
            let target_row = (ctx.current_row_idx as isize + page_size as isize)
                .clamp(0, ctx.max_idx as isize) as usize;
            update_scroll_state(&mut view, &ctx.path, target_row, None);
        }
    }

    fn top(&self) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            update_scroll_state(&mut view, &ctx.path, 0, None);
        }
    }

    fn bottom(&self) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            update_scroll_state(&mut view, &ctx.path, ctx.max_idx, None);
        }
    }

    fn row(&self) -> u32 {
        self.resolve_cursor().map_or(0, |c| c.row as u32)
    }

    fn row_count(&self) -> u32 {
        self.resolve_cursor().map_or(0, |c| c.row_count as u32)
    }

    fn hunk_index(&self) -> Option<u32> {
        self.resolve_cursor()
            .and_then(|c| c.hunk_index)
            .map(|i| i as u32)
    }

    fn hunk_count(&self) -> u32 {
        self.resolve_cursor().map_or(0, |c| c.hunk_count as u32)
    }

    fn kind(&self) -> String {
        self.resolve_cursor()
            .map(|c| c.kind.to_string())
            .unwrap_or_else(|| "none".into())
    }

    fn text(&self) -> String {
        self.resolve_cursor().map(|c| c.text).unwrap_or_default()
    }

    fn old_line(&self) -> u32 {
        self.resolve_cursor()
            .and_then(|c| c.old_line)
            .map_or(0, |i| i as u32)
    }

    fn new_line(&self) -> u32 {
        self.resolve_cursor()
            .and_then(|c| c.new_line)
            .map_or(0, |i| i as u32)
    }

    fn is_staged(&self) -> bool {
        self.resolve_cursor()
            .and_then(|c| c.is_staged)
            .unwrap_or(false)
    }
}

impl AppActionsHandler {
    /// Resolves the cursor context for the open file.
    fn resolve_cursor(&self) -> Option<CursorInfo> {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        cursor_info(&mut ui.file_view, &tree, &self.cache)
    }
}

impl ViewFileNavActionsHandler for AppActionsHandler {
    fn next_hunk(&self) {
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let last_height = view.last_height;
            if let Some(starts) = view.hunk_starts(&ctx.path) {
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
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let last_height = view.last_height;
            if let Some(starts) = view.hunk_starts(&ctx.path) {
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
        let mut view = &mut self.ui.lock().file_view;
        if let Some(ctx) = get_file_context(&mut view) {
            let mut target_logical = 0;
            if let Some(mapping) = view.line_mapping(&ctx.path) {
                target_logical = mapping.get(ctx.current_row_idx).copied().unwrap_or(0);
            }

            view.is_folded = !view.is_folded;
            view.mark_dirty();

            let next_selected = if let Some(crate::diff::DiffResult::Text(diff)) =
                self.cache.diffs.get(&ctx.path).as_deref()
            {
                let new_lines_len = diff.new_file_content.lines().count();
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
