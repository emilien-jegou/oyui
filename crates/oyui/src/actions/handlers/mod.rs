use crate::actions::*;
use crate::app::UiState;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::EventRegistry;
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;
use typed_builder::TypedBuilder;

pub mod analysis_handler;
pub mod clipboard;
pub mod global_handler;
pub mod history;
pub mod settings_handler;
pub mod theme_handler;
pub mod ui_handler;
pub mod view_handlers;

#[derive(TypedBuilder, Clone)]
pub struct AppActionsHandler {
    pub ui: Arc<Mutex<UiState>>,
    pub theme: Arc<RwLock<ThemeState>>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub base_path: Option<PathBuf>,
    pub algorithm: crate::cli::DiffAlgorithm,
    pub worker: Arc<EventRegistry>,
    pub color_mode: TerminalColorMode,
    /// Shared config-error cell; action failures surface through it.
    pub error: Arc<RwLock<Option<String>>>,
}

pub fn generate(actions_handler: AppActionsHandler) -> BoxedHandler {
    let theme_handler = theme_handler::AppThemeActionsHandler {
        theme: actions_handler.theme.clone(),
        ui: actions_handler.ui.clone(),
        cache: actions_handler.cache.clone(),
        color_mode: actions_handler.color_mode.clone(),
        worker: actions_handler.worker.clone(),
        error: actions_handler.error.clone(),
    };

    build_handler! {
        global: actions_handler.clone(),
        theme: theme_handler.clone(),
        settings: actions_handler.clone(),
        analysis: actions_handler.clone(),
        ui: actions_handler.clone(),
        view {
            tree: actions_handler.clone(),
            file: actions_handler.clone(),
        }
    }
    .build()
}
