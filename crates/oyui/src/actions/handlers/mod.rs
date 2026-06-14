use crate::actions::state::TuiState;
use crate::actions::*;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crate::view::View;
use crate::worker::EventRegistry;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub mod global_handler;
pub mod theme_handler;
pub mod view_handlers;

#[derive(Clone)]
pub struct AppActionsHandler {
    pub state: Arc<TuiState>,
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub view: View,
    pub right_path: PathBuf,
    pub worker: Arc<EventRegistry>,
}

pub fn generate(actions_handler: AppActionsHandler) -> BoxedHandler {
    build_handler! {
        global: actions_handler.clone(),
        theme: actions_handler.clone(),
        view {
            tree: actions_handler.clone(),
            file: actions_handler.clone(),
        }
    }
    .build()
}
