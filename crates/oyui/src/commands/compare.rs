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
    /// Whether confirming is allowed with unresolved conflicts.
    pub allow_unresolved: bool,
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
        allow_unresolved: false,
        view: args.view.clone(),
    };
    run(options, config_path, session).await
}

/// Runs the three-way merge editor (`oyui merge`).
///
/// The working file (the output target) is populated with the conflict markers
/// and then compared against the base, so conflicts render inline in the file
/// view alongside normal hunks.
pub async fn run_merge(
    options: &RunOptions,
    args: &MergeArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let output = args.output.clone().unwrap_or_else(|| args.right.clone());
    let read = |p: &PathBuf| std::fs::read_to_string(p).unwrap_or_default();

    // Keep existing markers; otherwise synthesize a merge from the three sides.
    let content = match std::fs::read_to_string(&output) {
        Ok(text) if crate::diff::ConflictedFile::is_conflicted(&text) => text,
        _ => {
            let merged = crate::diff::merge3::merge3(
                &read(&args.base),
                &read(&args.left),
                &read(&args.right),
                args.view.diff_algorithm,
            );
            let choices = vec![None; merged.conflict_count()];
            merged.resolve_optional(&choices)
        }
    };

    if let Err(e) = std::fs::write(&output, &content) {
        return Err(eyre::eyre!("failed to write merge target: {e}").into());
    }

    let session = Session {
        operation: Operation::Merge,
        base_path: Some(args.base.clone()),
        left_path: args.base.clone(),
        right_path: output.clone(),
        write_target: Some(output),
        allow_unresolved: args.allow_unresolved,
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
    ui_state.writable = session.write_target.is_some();
    let ui = Arc::new(Mutex::new(ui_state));

    detect_conflicts(&session, &ui);

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
        allow_unresolved: session.allow_unresolved,
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
        .allow_unresolved(session.allow_unresolved)
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

/// Loads the merge target's conflicts for inline resolution.
///
/// The working file was already populated (markers kept or synthesized) by
/// [`run_merge`], so parsing it yields the conflicts to display alongside hunks.
fn detect_conflicts(session: &Session, ui: &Arc<Mutex<UiState>>) {
    if session.operation != Operation::Merge {
        return;
    }
    let Some(target) = &session.write_target else {
        return;
    };
    if let Ok(content) = std::fs::read_to_string(target) {
        if let Some(conflicts) = crate::diff::ConflictedFile::parse(&content) {
            ui.lock().resolve = Some(crate::app::ui_state::ResolveState::new(conflicts));
        }
    }
}
