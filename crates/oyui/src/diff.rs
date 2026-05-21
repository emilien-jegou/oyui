use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub before_lines: Range<usize>,
    pub after_lines: Range<usize>,
}

#[derive(Debug)]
pub struct FileDiff {
    pub old_text: Arc<str>,
    pub new_text: Arc<str>,
    pub hunks: Vec<Hunk>,
    pub line_selections: Vec<bool>,
}
