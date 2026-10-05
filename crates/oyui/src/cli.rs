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
pub struct Args {
    #[clap(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// View or edit a two-way diff (git difftool, jj diff/diffedit).
    ///
    /// `left` is the old side, `right` the new side. Confirming writes the
    /// selected result back to `right`.
    Diff(DiffArgs),

    /// Resolve a three-way merge or conflict (jj resolve, git mergetool).
    ///
    /// `base`, `left` and `right` are the common ancestor and the two sides.
    /// Confirming writes the merged result to `--output` (defaults to `right`).
    Merge(MergeArgs),

    /// Run the LSP
    LanguageServer,
}

/// Options shared by the diff/merge views.
#[derive(clap::Args, Debug, Clone)]
pub struct ViewArgs {
    #[arg(long = "diff-algorithm", default_value = "histogram")]
    pub diff_algorithm: DiffAlgorithm,

    #[arg(long = "scrolloff", default_value = "2")]
    pub scrolloff: usize,

    #[arg(long = "context-lines", default_value = "4")]
    pub context_lines: usize,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DiffArgs {
    /// Old side.
    pub left: PathBuf,
    /// New side; receives the confirmed result unless `--no-write` is set.
    pub right: PathBuf,

    /// Inspect only: never write the result back.
    #[arg(long = "no-write")]
    pub no_write: bool,

    #[command(flatten)]
    pub view: ViewArgs,
}

#[derive(clap::Args, Debug, Clone)]
pub struct MergeArgs {
    /// Common ancestor.
    pub base: PathBuf,
    /// First side ("ours"/"local").
    pub left: PathBuf,
    /// Second side ("theirs"/"remote").
    pub right: PathBuf,

    /// Where to write the merged result; defaults to `right`.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    #[command(flatten)]
    pub view: ViewArgs,
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
