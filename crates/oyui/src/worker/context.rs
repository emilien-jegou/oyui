use crate::{
    actions::state::TuiState, cli::DiffAlgorithm, diff_cache::DiffCache, syntax::SyntaxEngine, tree::FileTree, view::View
};
use oyui_tasker::TaskerProvide;
use parking_lot::RwLock;
use std::{path::PathBuf, sync::Arc};
use typed_builder::TypedBuilder;

#[derive(TypedBuilder, TaskerProvide, Clone)]
pub struct AppWorkerContext {
    pub syntax_engine: SyntaxEngine,
    pub algorithm: DiffAlgorithm,

    pub tree: Arc<RwLock<FileTree>>,
    pub view: View,
    pub cache: DiffCache,
    pub config_error: Arc<RwLock<Option<String>>>,
    pub state: Arc<TuiState>,
    pub current_path: Arc<RwLock<Option<PathBuf>>>,
}
