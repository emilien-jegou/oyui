use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DiffAlgorithm {
    Histogram,
    Myers,
    #[clap(alias = "myers-minimal")]
    MyersMinimal,
    #[clap(alias = "experimental--syntax-aware")]
    SyntaxAware,
}

#[derive(Parser, Clone, Debug)]
#[command(name = "oyui", version, about, subcommand_required = true)]
pub struct Opts {
    #[clap(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Diff two directories
    Diff(DiffArgs),

    /// Run the LSP
    LanguageServer,
}

#[derive(Parser, Debug, Clone)]
pub struct DiffArgs {
    pub left: PathBuf,
    pub right: PathBuf,

    #[arg(short = 'b', long = "base")]
    pub base: Option<PathBuf>,

    #[arg(long = "diff-algorithm", default_value = "histogram")]
    pub diff_algorithm: DiffAlgorithm,

    #[arg(long = "scrolloff", default_value = "2")]
    pub scrolloff: usize,

    #[arg(long = "context-lines", default_value = "4")]
    pub context_lines: usize,
}

#[derive(Parser, Debug, Clone)]
pub struct CommonArgs {
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    #[arg(long = "flamegraph-enable")]
    pub flamegraph_enable: bool,

    #[arg(long = "flamegraph-save-path")]
    pub flamegraph_save_file: Option<PathBuf>,

    #[arg(long = "log-enable")]
    pub log_enable: bool,

    #[arg(long = "log-save-path")]
    pub log_save_path: Option<PathBuf>,

    #[arg(long = "log-console")]
    pub log_console: bool,
}
