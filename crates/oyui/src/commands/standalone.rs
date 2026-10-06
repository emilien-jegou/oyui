//! VCS-agnostic front-ends: `oyui`, `oyui diff`, `oyui interdiff`.
//!
//! Each command detects the enclosing repository and delegates to the native
//! VCS with oyui registered as the external tool.
use crate::app::Operation;
use crate::cli::{DiffCommandArgs, InterdiffArgs, ResolveArgs, SplitArgs, SquashArgs};
use crate::commands::{compare, CommandError, RunOptions};
use crate::vcs::tool::{ToolInvocation, ToolProgram};
use crate::vcs::{self, RepoKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn not_a_repo() -> CommandError {
    CommandError::Runtime("not inside a jj or git repository".into())
}

fn program(config_path: &Path) -> Result<ToolProgram, CommandError> {
    let exe = std::env::current_exe()?;
    Ok(ToolProgram {
        exe,
        config: Some(config_path.to_path_buf()),
    })
}

fn run(inv: ToolInvocation) -> Result<(), CommandError> {
    let status = inv.run()?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::ChildExit(status.code().unwrap_or(1)))
    }
}

/// `oyui`: open the current change of the detected repository.
pub fn run_current_change(_opts: &RunOptions, config_path: PathBuf) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    let program = program(&config_path)?;
    let inv = match repo.kind {
        RepoKind::Jujutsu => vcs::jujutsu::current_change(&program),
        RepoKind::Git => vcs::git::current_change(&program),
    };
    run(inv)
}

/// `oyui diff`: show the diff of the selected revisions.
pub fn run_diff(
    _opts: &RunOptions,
    args: &DiffCommandArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    let program = program(&config_path)?;
    let inv = match repo.kind {
        RepoKind::Jujutsu => vcs::jujutsu::diff(&program, args),
        RepoKind::Git => {
            vcs::git::diff(&program, args).map_err(|e| CommandError::Runtime(e.into()))?
        }
    };
    run(inv)
}

/// `oyui interdiff`: show how one revision's patch differs from another's.
pub fn run_interdiff(
    _opts: &RunOptions,
    args: &InterdiffArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    let program = program(&config_path)?;
    let inv = match repo.kind {
        RepoKind::Jujutsu => vcs::jujutsu::interdiff(&program, args),
        RepoKind::Git => vcs::git::interdiff(&program, args, &repo.root)
            .map_err(|e| CommandError::Runtime(e.into()))?,
    };
    run(inv)
}

/// `oyui resolve`: resolve the conflicts in the current change.
pub fn run_resolve(
    _opts: &RunOptions,
    args: &ResolveArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    let program = program(&config_path)?;
    let inv = match repo.kind {
        RepoKind::Jujutsu => vcs::jujutsu::resolve(&program, args),
        RepoKind::Git => vcs::git::resolve(&program, args),
    };
    run(inv)
}

/// `oyui split`: split the current change into two commits.
pub async fn run_split(
    opts: &RunOptions,
    args: &SplitArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    match repo.kind {
        RepoKind::Jujutsu => {
            let program = program(&config_path)?;
            run(vcs::jujutsu::split(&program, args))
        }
        RepoKind::Git => run_git_split(opts, args, config_path, repo.root).await,
    }
}

/// `oyui squash`: fold (part of) the current change into its parent.
pub async fn run_squash(
    opts: &RunOptions,
    args: &SquashArgs,
    config_path: PathBuf,
) -> Result<(), CommandError> {
    let repo = vcs::detect_cwd().ok_or_else(not_a_repo)?;
    match repo.kind {
        RepoKind::Jujutsu => {
            let program = program(&config_path)?;
            run(vcs::jujutsu::squash(&program, args))
        }
        RepoKind::Git => run_git_squash(opts, args, config_path, repo.root).await,
    }
}

/// Git squash: amend HEAD with the selected working-tree hunks.
async fn run_git_squash(
    opts: &RunOptions,
    args: &SquashArgs,
    config_path: PathBuf,
    root: PathBuf,
) -> Result<(), CommandError> {
    if args.from.is_some() || args.into.is_some() || args.keep_emptied {
        return Err(CommandError::Runtime(
            "git squash only folds the working-tree changes into HEAD".into(),
        ));
    }
    if !args.paths.is_empty() {
        return Err(CommandError::Runtime(
            "path-restricted squash is not supported by git yet".into(),
        ));
    }
    let plan = vcs::git_split::prepare(&root, None, None)?;
    let session = compare::Session {
        operation: Operation::Diff,
        base_path: None,
        left_path: plan.left_dir.clone(),
        right_path: plan.right_dir.clone(),
        write_target: Some(plan.write_dir.clone()),
        allow_unresolved: false,
        // Squash defaults to folding everything into HEAD; unstage to keep.
        default_staged: true,
        view: args.view.clone(),
    };
    let confirmed = Arc::new(AtomicBool::new(false));
    compare::run_session(opts, config_path, session, confirmed.clone()).await?;
    if !confirmed.load(Ordering::SeqCst) {
        eprintln!("squash cancelled; HEAD unchanged");
        return Ok(());
    }
    match vcs::git_split::finalize_squash(&plan)? {
        Some(outcome) => {
            let short = |id: &str| id.chars().take(10).collect::<String>();
            eprintln!(
                "squashed into HEAD ({} -> {}); undo with `git reset --hard {}`",
                short(&outcome.old),
                short(&outcome.new),
                short(&outcome.old),
            );
        }
        None => eprintln!("nothing selected; HEAD unchanged"),
    }
    Ok(())
}

/// Git split: run the diff editor against scratch trees, then commit.
async fn run_git_split(
    opts: &RunOptions,
    args: &SplitArgs,
    config_path: PathBuf,
    root: PathBuf,
) -> Result<(), CommandError> {
    if args.revision.is_some() {
        return Err(CommandError::Runtime(
            "git has no revisions; `oyui split` always splits the working-tree changes".into(),
        ));
    }
    if !args.paths.is_empty() {
        return Err(CommandError::Runtime(
            "path-restricted split is not supported by git yet".into(),
        ));
    }
    let plan = vcs::git_split::prepare(&root, args.message.clone(), args.message_second.clone())?;
    let session = compare::Session {
        operation: Operation::Diff,
        base_path: None,
        left_path: plan.left_dir.clone(),
        right_path: plan.right_dir.clone(),
        write_target: Some(plan.write_dir.clone()),
        allow_unresolved: false,
        default_staged: false,
        view: args.view.clone(),
    };
    let confirmed = Arc::new(AtomicBool::new(false));
    compare::run_session(opts, config_path, session, confirmed.clone()).await?;
    if !confirmed.load(Ordering::SeqCst) {
        eprintln!("split cancelled; no commits created");
        return Ok(());
    }
    let outcome = vcs::git_split::finalize(&plan)?;
    let short = |id: &str| id.chars().take(10).collect::<String>();
    eprintln!(
        "split into {} -> {} (previous HEAD saved at {}, undo with `git reset --hard {short_old}`)",
        short(&outcome.first),
        short(&outcome.second),
        vcs::git_split::BACKUP_REF,
        short_old = short(&outcome.old),
    );
    Ok(())
}
