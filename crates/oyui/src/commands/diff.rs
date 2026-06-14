use crate::actions::handlers::{self, AppActionsHandler};
use crate::actions::state::TuiState;
use crate::app::App;
use crate::cli::DiffArgs;
use crate::commands::{CommandError, RunOptions};
use crate::config::Config;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::view::file::FileViewData;
use crate::view::View;
use crate::worker::context::AppWorkerContext;
use crate::worker::EventRegistry;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn run_diff(
    options: &RunOptions,
    diff_args: &DiffArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let tree = Arc::new(RwLock::new(crate::tree::FileTree::default()));
    let cache = DiffCache::default();
    let config_error = Arc::new(RwLock::new(None));
    let current_path = Arc::new(RwLock::new(None));

    let file_view_data = FileViewData::new(options.color_mode.support_true_color());
    let view = View {
        current: Default::default(),
        tree_view: Default::default(),
        file_view: Arc::new(RwLock::new(file_view_data)),
    };
    let state = Arc::new(TuiState::new(&options.color_mode));

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .view(view.clone())
        .algorithm(diff_args.diff_algorithm)
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(config_error.clone())
        .state(state.clone())
        .current_path(current_path.clone())
        .build();

    let worker = Arc::new(EventRegistry::spawn(worker_context));

    let handler = handlers::generate(AppActionsHandler {
        state: state.clone(),
        tree: tree.clone(),
        cache: cache.clone(),
        view: view.clone(),
        worker: worker.clone(),
        right_path: diff_args.right.clone(),
        color_mode: options.color_mode.clone(),
    });

    let config = Config {
        path: config_path,
        error: config_error.clone(),
        handler: handler.clone(),
    };

    view.configure(diff_args.scrolloff, diff_args.context_lines);

    let mut app = App::builder()
        .worker(worker)
        .config(config)
        .base_path(diff_args.base.clone())
        .left_path(diff_args.left.clone())
        .right_path(diff_args.right.clone())
        .view(view)
        .tree(tree)
        .state(state)
        .cache(cache)
        .handler(handler)
        .current_path(current_path)
        .color_mode(options.color_mode.clone())
        .build();

    app.start().await?;
    Ok(())
}
