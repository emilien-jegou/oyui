//! Request to prewarm a file's diff and syntax while it is selected in the tree.
use std::path::PathBuf;

#[derive(Clone)]
pub struct FileHovered {
    pub path: PathBuf,
    pub generation: u64,
}
