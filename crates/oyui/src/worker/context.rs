use oyui_tasker::TaskerProvide;
use parking_lot::RwLock;
use std::sync::Arc;
use typed_builder::TypedBuilder;

use crate::{
    cli::DiffAlgorithm, diff_cache::DiffCache, syntax::SyntaxEngine, theme::ThemeState,
    tree::FileTree,
};

/// Context handed to listeners: shared state only, no UI types.
#[derive(TypedBuilder, TaskerProvide, Clone)]
pub struct AppWorkerContext {
    pub syntax_engine: SyntaxEngine,
    pub algorithm: DiffAlgorithm,

    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub config_error: Arc<RwLock<Option<String>>>,
    pub theme: Arc<RwLock<ThemeState>>,
}
