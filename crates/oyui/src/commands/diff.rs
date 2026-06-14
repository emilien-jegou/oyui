use crate::actions::handlers::{self, AppActionsHandler};
use crate::actions::state::TuiState;
use crate::app::App;
use crate::cli::{DiffArgs, Opts};
use crate::commands::CommandError;
use crate::commons::lazy::Lazy;
use crate::config::Config;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::view::View;
use crate::worker::context::AppWorkerContext;
use crate::worker::EventRegistry;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn run_diff(
    opts: &Opts,
    diff_args: &DiffArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let tree = Arc::new(RwLock::new(crate::tree::FileTree::default()));
    let cache = DiffCache::default();
    let config_error = Arc::new(RwLock::new(None));
    let syntax_theme = Arc::new(RwLock::new(Lazy::Uninitialized));
    let current_path = Arc::new(RwLock::new(None));

    let view = View::default();
    let state = Arc::new(TuiState::new("weywot"));

    let config = Config::new(config_path.clone());

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(diff_args.diff_algorithm)
        .config(opts.clone())
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(config_error.clone())
        .syntax_theme(syntax_theme.clone())
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
    });

    view.configure(diff_args.scrolloff, diff_args.context_lines);

    let mut app = App::builder()
        .worker(worker)
        .config_path(config_path)
        .config_error(config_error.clone())
        .base_path(diff_args.base.clone())
        .left_path(diff_args.left.clone())
        .right_path(diff_args.right.clone())
        .view(view)
        .tree(tree)
        .state(state)
        .cache(cache)
        .syntax_theme(syntax_theme)
        .handler(handler)
        .config(config)
        .current_path(current_path)
        .build();

    app.start().await?;
    Ok(())
}
