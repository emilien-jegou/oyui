use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
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

impl DiffAlgorithm {
    /// The CLI value name, for forwarding to a nested oyui.
    pub fn as_arg(self) -> &'static str {
        match self {
            DiffAlgorithm::Histogram => "histogram",
            DiffAlgorithm::Myers => "myers",
            DiffAlgorithm::MyersMinimal => "myers-minimal",
            DiffAlgorithm::SyntaxAware => "syntax-aware",
        }
    }
}

impl Default for DiffAlgorithm {
    fn default() -> Self {
        Self::Histogram
    }
}

#[derive(Parser, Clone, Debug)]
#[command(
    name = "oyui",
    version,
    about,
    subcommand_required = false,
    after_help = "Run without a COMMAND to open the current change of the enclosing\n\
                  Jujutsu or Git repository. The `jj`/`git` subcommands are the tool\n\
                  entrypoints those VCSs invoke via their merge-tool configuration."
)]
pub struct Args {
    #[clap(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Jujutsu integration tools, invoked by jj via merge-tools.oyui.
    Jj(JjArgs),

    /// Git integration tools, invoked by git via difftool/mergetool.oyui.
    Git(GitArgs),

    /// Show the diff of a change, using Jujutsu-style revision arguments.
    Diff(DiffCommandArgs),

    /// Show how the patch of one revision differs from another.
    Interdiff(InterdiffArgs),

    /// Split the current change into two commits.
    Split(SplitArgs),

    /// Resolve the conflicts in the current change.
    Resolve(ResolveArgs),

    /// Squash (part of) the current change into its parent.
    Squash(SquashArgs),

    /// Run the LSP
    LanguageServer,
}

/// Options for the VCS-agnostic `resolve`.
#[derive(ClapArgs, Debug, Clone)]
pub struct ResolveArgs {
    /// Only resolve conflicts in these paths.
    pub paths: Vec<String>,
}

/// Options for the VCS-agnostic `squash`.
#[derive(ClapArgs, Debug, Clone)]
pub struct SquashArgs {
    /// Move changes from this revision (defaults to the current change).
    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,

    /// Move changes into this revision (defaults to the parent).
    #[arg(short = 't', long = "into")]
    pub into: Option<String>,

    /// Keep the source change even if it becomes empty.
    #[arg(short = 'k', long = "keep-emptied")]
    pub keep_emptied: bool,

    /// Restrict the squash to these paths.
    pub paths: Vec<String>,

    #[command(flatten)]
    pub view: ViewArgs,
}

/// Options for the VCS-agnostic `split`.
#[derive(ClapArgs, Debug, Clone)]
pub struct SplitArgs {
    /// Message for the selected (first) commit.
    #[arg(short = 'm', long = "message")]
    pub message: Option<String>,

    /// Message for the remaining (second) commit.
    #[arg(long = "message-second")]
    pub message_second: Option<String>,

    /// Jujutsu revision to split (defaults to `@`; not supported by git).
    #[arg(short = 'r', long = "revision")]
    pub revision: Option<String>,

    /// Restrict the split to these paths.
    pub paths: Vec<String>,

    #[command(flatten)]
    pub view: ViewArgs,
}

/// Revision selection for the VCS-agnostic `diff`, mirroring `jj diff`.
#[derive(ClapArgs, Debug, Clone, Default)]
pub struct DiffCommandArgs {
    /// Show changes in these revisions (revset).
    #[arg(short = 'r', long = "revisions")]
    pub revisions: Option<String>,

    /// Show changes from this revision.
    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,

    /// Show changes to this revision.
    #[arg(short = 't', long = "to")]
    pub to: Option<String>,

    /// Restrict the diff to these paths.
    pub paths: Vec<String>,

    #[command(flatten)]
    pub view: ViewArgs,
}

/// Revision selection for the VCS-agnostic `interdiff`, mirroring `jj interdiff`.
#[derive(ClapArgs, Debug, Clone, Default)]
pub struct InterdiffArgs {
    /// The first revision to compare (default: `@`).
    #[arg(short = 'f', long = "from")]
    pub from: Option<String>,

    /// The second revision to compare (default: `@`).
    #[arg(short = 't', long = "to")]
    pub to: Option<String>,

    /// Restrict the interdiff to these paths.
    pub paths: Vec<String>,

    #[command(flatten)]
    pub view: ViewArgs,
}

/// Jujutsu merge-tool entrypoints.
#[derive(ClapArgs, Debug, Clone)]
pub struct JjArgs {
    #[command(subcommand)]
    pub tool: JjTool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum JjTool {
    /// Two-way diff editor: jj commit -i, squash -i, split, diffedit, restore -i.
    ///
    /// `left` is the original side, `right` the changed side; confirming writes
    /// the result back to `right`, which jj reads as the new change content.
    Edittool(DiffArgs),

    /// Read-only two-way diff viewer: jj diff --tool oyui, jj interdiff --tool.
    Difftool(DiffArgs),

    /// Three-way merge editor: jj resolve. Conflicts may be confirmed as-is.
    Mergetool(MergeArgs),
}

/// Git difftool/mergetool entrypoints.
#[derive(ClapArgs, Debug, Clone)]
pub struct GitArgs {
    #[command(subcommand)]
    pub tool: GitTool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum GitTool {
    /// Read-only two-way diff viewer: git difftool.
    Difftool(DiffArgs),

    /// Three-way merge editor: git mergetool. Resolution is required.
    Mergetool(MergeArgs),
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

impl Default for ViewArgs {
    fn default() -> Self {
        Self {
            diff_algorithm: DiffAlgorithm::default(),
            scrolloff: 2,
            context_lines: 4,
        }
    }
}

impl ViewArgs {
    /// Flags forwarding these view options to a nested `oyui`.
    pub fn forwarded(&self) -> Vec<String> {
        vec![
            "--diff-algorithm".to_string(),
            self.diff_algorithm.as_arg().to_string(),
            "--scrolloff".to_string(),
            self.scrolloff.to_string(),
            "--context-lines".to_string(),
            self.context_lines.to_string(),
        ]
    }
}

#[derive(clap::Args, Debug, Clone)]
pub struct DiffArgs {
    /// Old side.
    pub left: PathBuf,
    /// New side; receives the confirmed result in edit mode.
    pub right: PathBuf,

    /// Start with every hunk selected (used by `oyui squash`).
    #[arg(long = "start-staged", hide = true)]
    pub start_staged: bool,

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

#[derive(clap::Args, Debug, Clone)]
pub struct CommonArgs {
    #[arg(short = 'c', long = "config", global = true)]
    pub config: Option<PathBuf>,

    #[arg(long = "flamegraph-enable", global = true)]
    pub flamegraph_enable: bool,

    #[arg(long = "flamegraph-save-path", global = true)]
    pub flamegraph_save_file: Option<PathBuf>,

    #[arg(long = "log-enable", global = true)]
    pub log_enable: bool,

    #[arg(long = "log-save-path", global = true)]
    pub log_save_path: Option<PathBuf>,

    #[arg(long = "log-console", global = true)]
    pub log_console: bool,
}
