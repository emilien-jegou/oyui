use std::error::Error;
use std::io;
use std::path::PathBuf;

use crate::{
    cli::{Args, Commands, GitTool, JjTool},
    terminal_colors::TerminalColorMode,
};

pub mod compare;
pub mod standalone;

#[derive(Debug)]
pub enum CommandError {
    NoModifications,
    Aborted,
    /// A delegated VCS command exited non-zero; propagate its code.
    ChildExit(i32),
    Runtime(Box<dyn Error>),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::NoModifications => {
                write!(f, "No modifications found between directories.")
            }
            CommandError::Aborted => write!(f, "Application aborted."),
            CommandError::ChildExit(code) => write!(f, "VCS command exited with status {code}."),
            CommandError::Runtime(err) => write!(f, "{}", err),
        }
    }
}

impl Error for CommandError {}

impl From<io::Error> for CommandError {
    fn from(err: io::Error) -> Self {
        CommandError::Runtime(Box::new(err))
    }
}

impl From<Box<dyn Error>> for CommandError {
    fn from(err: Box<dyn Error>) -> Self {
        CommandError::Runtime(err)
    }
}

impl From<eyre::Report> for CommandError {
    fn from(err: eyre::Report) -> Self {
        CommandError::Runtime(err.into())
    }
}

pub struct RunOptions {
    pub args: Args,
    pub color_mode: TerminalColorMode,
}

pub async fn run(opts: RunOptions) -> Result<(), CommandError> {
    let config_path = opts.args.common.config.clone().unwrap_or_else(|| {
        dirs::config_dir()
            .map(|d| d.join("oyui/config.rn"))
            .unwrap_or_else(|| PathBuf::from(".config/oyui/config.rn"))
    });

    match opts.args.command {
        Some(Commands::Jj(ref jj)) => match jj.tool {
            JjTool::Edittool(ref args) => compare::run_diff(&opts, args, config_path, true).await,
            JjTool::Difftool(ref args) => compare::run_diff(&opts, args, config_path, false).await,
            JjTool::Mergetool(ref args) => compare::run_merge(&opts, args, config_path, true).await,
        },
        Some(Commands::Git(ref git)) => match git.tool {
            GitTool::Difftool(ref args) => compare::run_diff(&opts, args, config_path, false).await,
            GitTool::Mergetool(ref args) => {
                compare::run_merge(&opts, args, config_path, false).await
            }
        },
        Some(Commands::Diff(ref args)) => standalone::run_diff(&opts, args, config_path),
        Some(Commands::Interdiff(ref args)) => standalone::run_interdiff(&opts, args, config_path),
        Some(Commands::Split(ref args)) => standalone::run_split(&opts, args, config_path).await,
        Some(Commands::Resolve(ref args)) => standalone::run_resolve(&opts, args, config_path),
        Some(Commands::Squash(ref args)) => standalone::run_squash(&opts, args, config_path).await,
        Some(Commands::LanguageServer) => crate::script::language_server::run_lsp().await,
        None => standalone::run_current_change(&opts, config_path),
    }
}
