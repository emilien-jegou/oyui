//! Shared bootstrap for the `diff` and `merge` operations.

use crate::actions::handlers::{self, AppActionsHandler};
use crate::app::{App, Operation, SessionContext, UiState};
use crate::cli::{DiffArgs, MergeArgs, ViewArgs};
use crate::commands::{CommandError, RunOptions};
use crate::config::Config;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::theme::ThemeState;
use crate::worker::context::AppWorkerContext;
use crate::worker::EventRegistry;
use parking_lot::{Mutex, RwLock};
use std::io;
use std::path::{Path, PathBuf};
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
    /// Start every discovered change staged (used by `oyui squash`).
    pub default_staged: bool,
    pub view: ViewArgs,
}

/// Runs a two-way diff session.
///
/// `writable` distinguishes the diff editor (`oyui jj edittool`) from the
/// read-only viewer (`oyui jj difftool`, `oyui git difftool`).
pub async fn run_diff(
    options: &RunOptions,
    args: &DiffArgs,
    config_path: PathBuf,
    writable: bool,
) -> Result<(), CommandError> {
    let session = Session {
        operation: Operation::Diff,
        base_path: None,
        left_path: args.left.clone(),
        right_path: args.right.clone(),
        write_target: writable.then(|| args.right.clone()),
        allow_unresolved: false,
        default_staged: args.start_staged,
        view: args.view.clone(),
    };
    match run_session(options, config_path, session, None).await? {
        SessionOutcome::Confirmed => Ok(()),
        SessionOutcome::Aborted => {
            eprintln!("merge cancelled; output left untouched");
            Err(CommandError::Aborted)
        }
    }
}

/// Reads one side of a merge, reporting which side failed.
fn read_side(label: &str, path: &Path) -> Result<String, CommandError> {
    std::fs::read_to_string(path).map_err(|err| {
        CommandError::Runtime(
            io::Error::new(
                err.kind(),
                format!("failed to read merge {label} '{}': {err}", path.display()),
            )
            .into(),
        )
    })
}

/// Builds the merged content when the target has no markers yet.
fn synthesize_merge(
    base: &str,
    left: &str,
    right: &str,
    allow_unresolved: bool,
    algorithm: crate::cli::DiffAlgorithm,
) -> String {
    let merged = crate::diff::merge3::merge3(base, left, right, algorithm);
    if allow_unresolved {
        merged.to_jj_snapshot()
    } else {
        let choices = vec![None; merged.conflict_count()];
        merged.resolve_optional(&choices)
    }
}

/// Runs a three-way merge session.
///
/// The merged content is staged in a temporary file and compared against the
/// base, so conflicts render inline in the file view alongside normal hunks.
/// The real output target is only written when the user confirms; quitting
/// aborts (non-zero exit) and leaves the target byte-for-byte untouched.
/// `allow_unresolved` follows the integration: jj stores conflicts structurally
/// and permits confirming them, git does not.
pub async fn run_merge(
    options: &RunOptions,
    args: &MergeArgs,
    config_path: PathBuf,
    allow_unresolved: bool,
) -> Result<(), CommandError> {
    let output = args.output.clone().unwrap_or_else(|| args.right.clone());
    let base = read_side("base", &args.base)?;
    let left = read_side("left", &args.left)?;
    let right = read_side("right", &args.right)?;

    // Keep existing markers; otherwise synthesize a merge from the three sides.
    // For jj (marker-less `$base`/`$left`/`$right` snapshots) the synthesis uses
    // jj snapshot-style markers so the view — and any unresolved write-back —
    // stays in jj's representation instead of converting to git markers.
    //
    // A missing target is normal (the VCS expects us to create it); any other
    // read failure is fatal rather than being mistaken for an empty file.
    let content = match std::fs::read_to_string(&output) {
        Ok(text) if crate::diff::ConflictedFile::is_conflicted(&text) => text,
        Ok(_) => synthesize_merge(
            &base,
            &left,
            &right,
            allow_unresolved,
            args.view.diff_algorithm,
        ),
        Err(err) if err.kind() == io::ErrorKind::NotFound => synthesize_merge(
            &base,
            &left,
            &right,
            allow_unresolved,
            args.view.diff_algorithm,
        ),
        Err(err) => {
            return Err(CommandError::Runtime(
                io::Error::new(
                    err.kind(),
                    format!("failed to read merge target '{}': {err}", output.display()),
                )
                .into(),
            ));
        }
    };

    // Stage the merged view next to a copy of the target's file name so the
    // file view keeps showing the real names, but never touch the target until
    // confirmation. The directory is removed on abort and on success alike.
    let staging = tempfile::Builder::new()
        .prefix("oyui-merge-")
        .tempdir()
        .map_err(|err| CommandError::Runtime(Box::new(err)))?;
    let staged_output = staging.path().join(
        output
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("merged")),
    );
    std::fs::write(&staged_output, &content).map_err(|err| {
        CommandError::Runtime(
            io::Error::new(
                err.kind(),
                format!(
                    "failed to stage merge result for '{}': {err}",
                    output.display()
                ),
            )
            .into(),
        )
    })?;

    let session = Session {
        operation: Operation::Merge,
        base_path: Some(args.base.clone()),
        left_path: args.base.clone(),
        right_path: staged_output,
        write_target: Some(output),
        allow_unresolved,
        default_staged: false,
        view: args.view.clone(),
    };
    match run_session(options, config_path, session, Some(&content)).await? {
        SessionOutcome::Confirmed => Ok(()),
        SessionOutcome::Aborted => {
            eprintln!("merge cancelled; output left untouched");
            Err(CommandError::Aborted)
        }
    }
}

/// How a session ended: the user either confirmed the result or backed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutcome {
    /// The user confirmed the pending result.
    Confirmed,
    /// The user quit without confirming.
    Aborted,
}

/// Runs a prepared session; returns how the session ended.
///
/// `merge_text` is the synthesized merge content, when the session is a merge;
/// it lets [`detect_conflicts`] parse the exact bytes being displayed instead
/// of re-reading them from disk.
pub(crate) async fn run_session(
    options: &RunOptions,
    config_path: PathBuf,
    session: Session,
    merge_text: Option<&str>,
) -> Result<SessionOutcome, CommandError> {
    let tree = Arc::new(RwLock::new(crate::tree::FileTree::default()));
    let cache = DiffCache::default();
    let config_error = Arc::new(RwLock::new(None));
    let theme = Arc::new(RwLock::new(ThemeState::new(&options.color_mode)));
    let mut ui_state = UiState::new(options.color_mode.support_true_color());
    ui_state.configure(session.view.scrolloff, session.view.context_lines);
    ui_state.writable = session.write_target.is_some();
    let ui = Arc::new(Mutex::new(ui_state));

    let worker_context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(session.view.diff_algorithm)
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(config_error.clone())
        .theme(theme.clone())
        .cpu(crate::worker::cpu::bounded_pool("oyui-worker")?)
        .build();

    let worker = Arc::new(EventRegistry::spawn(worker_context));

    // The single owner of every shared handle and session static; the App,
    // the action handlers, and the config all reach them through one `Arc`.
    let ctx = Arc::new(
        SessionContext::builder()
            .ui(ui)
            .theme(theme)
            .tree(tree)
            .cache(cache)
            .worker(worker.clone())
            .error(config_error.clone())
            .confirmed(Arc::new(std::sync::atomic::AtomicBool::new(false)))
            .operation(session.operation)
            .left_path(session.left_path.clone())
            .right_path(session.right_path.clone())
            .base_path(session.base_path.clone())
            .write_target(session.write_target.clone())
            .allow_unresolved(session.allow_unresolved)
            .algorithm(session.view.diff_algorithm)
            .color_mode(options.color_mode.clone())
            .default_staged(session.default_staged)
            .build(),
    );

    detect_conflicts(&ctx, merge_text);

    let handler = handlers::generate(AppActionsHandler::builder().ctx(ctx.clone()).build());

    let config = Config {
        path: config_path,
        host: crate::script::RuneHost::new(),
        keybinds: crate::actions::keybinds::default_keybinds(),
        handler: handler.clone(),
        ctx: ctx.clone(),
    };

    let mut app = App::builder()
        .ctx(ctx.clone())
        .config(config)
        .handler(handler)
        .build();

    app.start().await?;
    Ok(if ctx.confirmed.load(std::sync::atomic::Ordering::SeqCst) {
        SessionOutcome::Confirmed
    } else {
        SessionOutcome::Aborted
    })
}

/// Loads the merge content's conflicts for inline resolution.
///
/// The bytes passed to [`run_session`] are the exact synthesized merge, so
/// parsing them avoids a second disk read of the staged copy.
fn detect_conflicts(ctx: &SessionContext, content: Option<&str>) {
    if ctx.operation != Operation::Merge {
        return;
    }
    let Some(content) = content else {
        return;
    };
    if let Some(conflicts) = crate::diff::ConflictedFile::parse(content) {
        ctx.ui.lock().resolve = Some(crate::app::ui_state::ResolveState::new(conflicts));
    }
}
