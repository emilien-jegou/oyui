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
}
