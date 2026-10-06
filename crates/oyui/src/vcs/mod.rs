//! Version-control detection and backend command construction.
pub mod detect;
pub mod git;
pub mod git_split;
pub mod jujutsu;
pub mod tool;

pub use detect::{detect, detect_cwd, Repo, RepoKind};
