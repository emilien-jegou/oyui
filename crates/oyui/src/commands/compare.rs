//! Shared bootstrap for the `diff` and `merge` operations.

use crate::actions::handlers::{self, AppActionsHandler};
use crate::app::{App, Operation, UiState};
use crate::cli::{DiffArgs, MergeArgs, ViewArgs};
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

/// Everything a session needs, independent of how it was requested.
pub struct Session {
    pub operation: Operation,
    pub base_path: Option<PathBuf>,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    /// Destination for the confirmed result; `None` means read-only.
    pub write_target: Option<PathBuf>,
    pub view: ViewArgs,
}

/// Runs the two-way diff editor (`oyui diff`).
pub async fn run_diff(
    options: &RunOptions,
    args: &DiffArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let session = Session {
        operation: Operation::Diff,
        base_path: None,
        left_path: args.left.clone(),
        right_path: args.right.clone(),
        write_target: (!args.no_write).then(|| args.right.clone()),
        view: args.view.clone(),
    };
    run(options, config_path, session).await
}

/// Runs the three-way merge editor (`oyui merge`).
pub async fn run_merge(
    options: &RunOptions,
    args: &MergeArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let output = args.output.clone().unwrap_or_else(|| args.right.clone());
    let session = Session {
        operation: Operation::Merge,
        base_path: Some(args.base.clone()),
        left_path: args.left.clone(),
        right_path: args.right.clone(),
        write_target: Some(output),
        view: args.view.clone(),
    };
    run(options, config_path, session).await
}

async fn run(
    options: &RunOptions,
    config_path: PathBuf,
    session: Session,
) -> Result<(), CommandError> {
    let tree = Arc::new(RwLock::new(crate::tree::FileTree::default()));
    let cache = DiffCache::default();
    let config_error = Arc::new(RwLock::new(None));
    let theme = Arc::new(RwLock::new(ThemeState::new(&options.color_mode)));
    let mut ui_state = UiState::new(options.color_mode.support_true_color());
    ui_state.configure(session.view.scrolloff, session.view.context_lines);
    let ui = Arc::new(Mutex::new(ui_state));

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(session.view.diff_algorithm)
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
        operation: session.operation,
        left_path: session.left_path.clone(),
        right_path: session.right_path.clone(),
        base_path: session.base_path.clone(),
        write_target: session.write_target.clone(),
        algorithm: session.view.diff_algorithm,
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
        .operation(session.operation)
        .base_path(session.base_path.clone())
        .left_path(session.left_path.clone())
        .right_path(session.right_path.clone())
        .write_target(session.write_target.clone())
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
