use crate::actions::state::TuiState;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crate::view::View;
use parking_lot::RwLock;
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
}

pub fn generate_handler(
    state: Arc<RwLock<TuiState>>,
    tree: Arc<RwLock<FileTree>>,
    cache: Arc<RwLock<DiffCache>>,
    view: View,
) -> BoxedHandler {
    let handler = AppActionsHandler {
        state,
        tree,
        cache,
        view,
    };
    Handler {
        global: handler.clone(),
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
    }
    .build()
}

pub fn dispatch_action(action: &Action, handler: &AppActionsHandler) {
    match &action.0 {
        Actions::global(act) => match act {
            GlobalActions::quit => GlobalActionsHandler::quit(handler),
            GlobalActions::confirm => GlobalActionsHandler::confirm(handler),
            GlobalActions::open_command_mode => GlobalActionsHandler::open_command_mode(handler),
        },
        Actions::view(act) => match act {
            ViewActions::tree(tree_act) => match tree_act {
                ViewTreeActions::open_selected => ViewTreeActionsHandler::open_selected(handler),
                ViewTreeActions::open_file(path) => {
                    ViewTreeActionsHandler::open_file(handler, path.clone())
                }
                ViewTreeActions::cursor(cursor_act) => match cursor_act {
                    ViewTreeCursorActions::up(val) => {
                        ViewTreeCursorActionsHandler::up(handler, *val)
                    }
                    ViewTreeCursorActions::down(val) => {
                        ViewTreeCursorActionsHandler::down(handler, *val)
                    }
                    ViewTreeCursorActions::half_page_up => {
                        ViewTreeCursorActionsHandler::half_page_up(handler)
                    }
                    ViewTreeCursorActions::half_page_down => {
                        ViewTreeCursorActionsHandler::half_page_down(handler)
                    }
                    ViewTreeCursorActions::top => ViewTreeCursorActionsHandler::top(handler),
                    ViewTreeCursorActions::bottom => ViewTreeCursorActionsHandler::bottom(handler),
                },
                ViewTreeActions::directory(dir_act) => match dir_act {
                    ViewTreeDirectoryActions::expand => {
                        ViewTreeDirectoryActionsHandler::expand(handler)
                    }
                    ViewTreeDirectoryActions::collapse => {
                        ViewTreeDirectoryActionsHandler::collapse(handler)
                    }
                },
                ViewTreeActions::staging(staging_act) => match staging_act {
                    ViewTreeStagingActions::toggle_selected => {
                        ViewTreeStagingActionsHandler::toggle_selected(handler)
                    }
                    ViewTreeStagingActions::invert => {
                        ViewTreeStagingActionsHandler::invert(handler)
                    }
                },
            },
            ViewActions::file(file_act) => match file_act {
                ViewFileActions::close => ViewFileActionsHandler::close(handler),
                ViewFileActions::scroll(scroll_act) => match scroll_act {
                    ViewFileScrollActions::left(val) => {
                        ViewFileScrollActionsHandler::left(handler, *val)
                    }
                    ViewFileScrollActions::right(val) => {
                        ViewFileScrollActionsHandler::right(handler, *val)
                    }
                },
                ViewFileActions::cursor(cursor_act) => match cursor_act {
                    ViewFileCursorActions::up(val) => {
                        ViewFileCursorActionsHandler::up(handler, *val)
                    }
                    ViewFileCursorActions::down(val) => {
                        ViewFileCursorActionsHandler::down(handler, *val)
                    }
                    ViewFileCursorActions::half_page_up => {
                        ViewFileCursorActionsHandler::half_page_up(handler)
                    }
                    ViewFileCursorActions::half_page_down => {
                        ViewFileCursorActionsHandler::half_page_down(handler)
                    }
                    ViewFileCursorActions::top => ViewFileCursorActionsHandler::top(handler),
                    ViewFileCursorActions::bottom => ViewFileCursorActionsHandler::bottom(handler),
                },
                ViewFileActions::nav(nav_act) => match nav_act {
                    ViewFileNavActions::next_hunk => ViewFileNavActionsHandler::next_hunk(handler),
                    ViewFileNavActions::prev_hunk => ViewFileNavActionsHandler::prev_hunk(handler),
                },
                ViewFileActions::staging(staging_act) => match staging_act {
                    ViewFileStagingActions::toggle => {
                        ViewFileStagingActionsHandler::toggle(handler)
                    }
                    ViewFileStagingActions::toggle_hunk(val) => {
                        ViewFileStagingActionsHandler::toggle_hunk(handler, *val)
                    }
                    ViewFileStagingActions::split => ViewFileStagingActionsHandler::split(handler),
                    ViewFileStagingActions::invert => {
                        ViewFileStagingActionsHandler::invert(handler)
                    }
                },
                ViewFileActions::fold(fold_act) => match fold_act {
                    ViewFileFoldActions::toggle => ViewFileFoldActionsHandler::toggle(handler),
                },
            },
        },
        _ => {}
    }
}
