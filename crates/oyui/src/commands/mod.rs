use std::error::Error;
use std::io;
use std::path::PathBuf;

use crate::cli::{Commands, Opts};

pub mod diff;
pub mod language_server;

#[derive(Debug)]
pub enum CommandError {
    NoModifications,
    Aborted,
    Runtime(Box<dyn Error>),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::NoModifications => {
                write!(f, "No modifications found between directories.")
            }
            CommandError::Aborted => write!(f, "Application aborted."),
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

pub async fn run(opts: Opts) -> Result<(), CommandError> {
    let config_path = opts.common.config.clone().unwrap_or_else(|| {
        dirs::config_dir()
            .map(|d| d.join("oyui/config.rn"))
            .unwrap_or_else(|| PathBuf::from(".config/oyui/config.rn"))
    });

    match opts.command {
        Commands::Diff(ref diff_args) => diff::run_diff(&opts, diff_args, config_path).await,
        Commands::LanguageServer => language_server::run_lsp().await,
    }
}
