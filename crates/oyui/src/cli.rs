use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "oyui")]
pub struct Opts {
    /// Diff mode flag (passed by jj)
    #[arg(short = 'd', long = "diff")]
    pub diff: bool,

    /// Left-hand file (old)
    pub left: PathBuf,

    /// Right-hand file (new)
    pub right: PathBuf,

    /// MERGETOOL ONLY: The common ancestor
    #[arg(short = 'b', long = "base")]
    pub base: Option<PathBuf>,

    /// MERGETOOL ONLY: Write resolved result here
    #[arg(short = 'o', long = "output", requires = "base")]
    pub output: Option<PathBuf>,
}
