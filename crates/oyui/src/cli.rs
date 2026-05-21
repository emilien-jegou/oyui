use clap::Parser;
use std::path::PathBuf;

#[derive(Clone, Debug, Parser)]
#[command(name = "oyui")]
pub struct Opts {
    /// Left-hand directory (old)
    pub left: PathBuf,

    /// Right-hand directory (new)
    pub right: PathBuf,

    /// MERGETOOL ONLY: The common ancestor
    #[arg(short = 'b', long = "base")]
    pub base: Option<PathBuf>,

    /// Enable flamegraph tracing
    #[arg(long = "flamegraph-enable")]
    pub flamegraph_enable: bool,

    /// Override the default path to save the flamegraph
    #[arg(long = "flamegraph-save-path")]
    pub flamegraph_save_file: Option<PathBuf>,

    /// Enable file logging
    #[arg(long = "log-enable")]
    pub log_enable: bool,

    /// Enable file logging and optionally specify the save path
    #[arg(long = "log-save-path")]
    pub log_save_path: Option<PathBuf>,

    /// Enable console logging (will automatically suspend while the TUI is drawn)
    #[arg(long = "log-console")]
    pub log_console: bool,
}
