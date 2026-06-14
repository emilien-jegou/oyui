use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

use crate::app::App;
use crate::cli::{DiffArgs, Opts};
use crate::commands::CommandError;
use crate::commons::lazy::Lazy;
use crate::syntax::SyntaxEngine;
use crate::view::View;
use crate::worker::context::AppWorkerContext;
use crate::worker::EventRegistry;

pub async fn run_diff(
    opts: &Opts,
    diff_args: &DiffArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let tree = Arc::new(RwLock::new(crate::tree::FileTree::default()));
    let cache = Arc::new(RwLock::new(crate::diff_cache::DiffCache::default()));
    let config_error = Arc::new(RwLock::new(None));
    let syntax_theme = Arc::new(RwLock::new(Lazy::Uninitialized));
    let current_path = Arc::new(RwLock::new(None));
    let inline_diff = Arc::new(RwLock::new(true));

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(diff_args.diff_algorithm)
        .inline_diff(inline_diff.clone())
        .config(opts.clone())
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(config_error.clone())
        .syntax_theme(syntax_theme.clone())
        .current_path(current_path.clone())
        .build();

    let worker = EventRegistry::spawn(worker_context);

    let view = View::default();
    view.configure(diff_args.scrolloff, diff_args.context_lines);

    let mut app = App::builder()
        .worker(worker)
        .inline_diff(inline_diff)
        .config_path(config_path)
        .config_error(config_error)
        .base_path(diff_args.base.clone())
        .left_path(diff_args.left.clone())
        .right_path(diff_args.right.clone())
        .view(view)
        .tree(tree)
        .cache(cache)
        .syntax_theme(syntax_theme)
        .current_path(current_path)
        .build();

    app.start().await?;
    Ok(())
}
