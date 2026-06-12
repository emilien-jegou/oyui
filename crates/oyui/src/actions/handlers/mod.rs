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

    build_handler! {
        global: handler.clone(),
        theme: handler.clone(),
        view {
            tree: handler.clone(),
            file: handler.clone(),
        }
    }.build()
}
