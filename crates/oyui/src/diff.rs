use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffStats {
    Text { insertions: usize, deletions: usize },
    Binary { bytes: isize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub before_lines: Range<usize>,
    pub after_lines: Range<usize>,
}

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub old_text: Arc<str>,
    pub new_text: Arc<str>,
    pub hunks: Vec<Hunk>,
    pub line_selections: Vec<bool>,
}

#[derive(Debug, Clone)]
pub enum DiffResult {
    Text(FileDiff),
    Empty,
    Binary {
        size: u64,
        mime: String,
        ext: String,
    },
    TooLarge(u64),
    Error(String),
}
