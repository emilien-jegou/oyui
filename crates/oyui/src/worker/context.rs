use crate::{
    cli::{DiffAlgorithm, Opts},
    commons::lazy::Lazy,
    diff_cache::DiffCache,
    syntax::SyntaxEngine,
    tree::FileTree,
};
use oyui_tasker::TaskerProvide;
use parking_lot::RwLock;
use std::{path::PathBuf, sync::Arc};
use typed_builder::TypedBuilder;

#[derive(TypedBuilder, TaskerProvide, Clone)]
pub struct AppWorkerContext {
    pub syntax_engine: SyntaxEngine,
    pub config: Opts,
    pub algorithm: DiffAlgorithm,

    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub config_error: Arc<RwLock<Option<String>>>,
    pub syntax_theme: Arc<RwLock<Lazy<syntect::highlighting::Theme>>>,
    pub current_path: Arc<RwLock<Option<PathBuf>>>,
}
