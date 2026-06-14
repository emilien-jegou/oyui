use crate::actions::state::TuiState;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::tree::FileTree;
use crate::view::View;
use crate::worker::EventRegistry;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use typed_builder::TypedBuilder;

pub mod global_handler;
pub mod theme_handler;
pub mod view_handlers;

#[derive(TypedBuilder, Clone)]
pub struct AppActionsHandler {
    pub state: Arc<TuiState>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub view: View,
    pub right_path: PathBuf,
    pub worker: Arc<EventRegistry>,
    pub color_mode: TerminalColorMode,
}

pub fn generate(actions_handler: AppActionsHandler) -> BoxedHandler {
    let theme_handler = theme_handler::AppThemeActionsHandler {
        state: actions_handler.state.clone(),
        view: actions_handler.view.clone(),
        cache: actions_handler.cache.clone(),
        color_mode: actions_handler.color_mode.clone(),
        worker: actions_handler.worker.clone(),
    };

    build_handler! {
        global: actions_handler.clone(),
        theme: theme_handler.clone(),
        view {
            tree: actions_handler.clone(),
            file: actions_handler.clone(),
        }
    }
    .build()
}
