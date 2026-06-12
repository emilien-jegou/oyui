use crate::actions::state::TuiState;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crate::view::View;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub mod global_handler;
pub mod theme_handler;
pub mod view_handlers;

#[derive(Clone)]
pub struct AppActionsHandler {
    pub state: Arc<RwLock<TuiState>>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: Arc<RwLock<DiffCache>>,
    pub view: View,
    pub right_path: PathBuf,
}

pub fn generate(
    state: Arc<RwLock<TuiState>>,
    tree: Arc<RwLock<FileTree>>,
    cache: Arc<RwLock<DiffCache>>,
    view: View,
    right_path: PathBuf,
) -> BoxedHandler {
    let handler = AppActionsHandler {
        state,
        tree,
        cache,
        view,
        right_path,
    };

    Handler {
        global: handler.clone(),
        global_confirm_merge_window_enabled: handler.clone(),
        theme: handler.clone(),
        theme_bg: handler.clone(),
        theme_fg: handler.clone(),
        theme_cursor_bg: handler.clone(),
        theme_dim: handler.clone(),
        theme_staged: handler.clone(),
        theme_unstaged: handler.clone(),
        theme_partial: handler.clone(),
        theme_dir: handler.clone(),
        theme_cmd: handler.clone(),
        theme_add_bg: handler.clone(),
        theme_del_bg: handler.clone(),
        theme_add_fg: handler.clone(),
        theme_del_fg: handler.clone(),
        theme_file_staged_highlight: handler.clone(),
        theme_file_staged_highlight_opacity: handler.clone(),
        theme_file_change_highlight: handler.clone(),
        theme_file_change_highlight_opacity: handler.clone(),
        theme_char_hunk_split: handler.clone(),
        theme_char_hunk_split_color: handler.clone(),
        theme_char_line_split: handler.clone(),
        theme_char_line_split_color: handler.clone(),
        theme_char_indicator: handler.clone(),
        theme_char_add_sign: handler.clone(),
        theme_char_del_sign: handler.clone(),
        view_file: handler.clone(),
        view_file_scroll: handler.clone(),
        view_file_cursor: handler.clone(),
        view_file_nav: handler.clone(),
        view_file_staging: handler.clone(),
        view_file_fold: handler.clone(),
        view_tree: handler.clone(),
        view_tree_cursor: handler.clone(),
        view_tree_directory: handler.clone(),
        view_tree_staging: handler.clone(),
        theme_char_trailing_space_fg: handler.clone(),
        theme_char_tab_fg: handler.clone(),
        theme_char_scroll_fg: handler.clone(),
        theme_char_trailing_space: handler.clone(),
        theme_char_tab: handler.clone(),
        theme_char_scroll_both: handler.clone(),
        theme_char_scroll_left: handler.clone(),
        theme_char_scroll_right: handler.clone(),
    }
    .build()
}
