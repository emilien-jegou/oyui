use crate::actions::handlers::{self, AppActionsHandler};
use crate::app::{App, UiState};
use crate::cli::DiffArgs;
use crate::commands::{CommandError, RunOptions};
use crate::config::Config;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::theme::ThemeState;
use crate::worker::context::AppWorkerContext;
use crate::worker::EventRegistry;
use parking_lot::{Mutex, RwLock};
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
    let theme = Arc::new(RwLock::new(ThemeState::new(&options.color_mode)));
    let mut ui_state = UiState::new(options.color_mode.support_true_color());
    ui_state.configure(diff_args.scrolloff, diff_args.context_lines);
    let ui = Arc::new(Mutex::new(ui_state));

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(diff_args.diff_algorithm)
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(config_error.clone())
        .theme(theme.clone())
        .build();

    let worker = Arc::new(EventRegistry::spawn(worker_context));

    let handler = handlers::generate(AppActionsHandler {
        ui: ui.clone(),
        theme: theme.clone(),
        tree: tree.clone(),
        cache: cache.clone(),
        worker: worker.clone(),
        left_path: diff_args.left.clone(),
        right_path: diff_args.right.clone(),
        base_path: diff_args.base.clone(),
        algorithm: diff_args.diff_algorithm,
        color_mode: options.color_mode.clone(),
        error: config_error.clone(),
    });

    let config = Config {
        path: config_path,
        error: config_error.clone(),
        handler: handler.clone(),
        keybinds: crate::actions::keybinds::default_keybinds(),
        host: crate::script::RuneHost::new(),
        worker: worker.clone(),
    };

    let mut app = App::builder()
        .worker(worker)
        .config(config)
        .base_path(diff_args.base.clone())
        .left_path(diff_args.left.clone())
        .right_path(diff_args.right.clone())
        .tree(tree)
        .theme(theme)
        .ui(ui)
        .cache(cache)
        .handler(handler)
        .color_mode(options.color_mode.clone())
        .build();

    app.start().await?;
    Ok(())
}
