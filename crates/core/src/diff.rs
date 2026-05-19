use std::ops::Range;
use std::sync::Arc;
use syntect::highlighting::Style as SyntectStyle;

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
    pub highlighted_new: Vec<Vec<(SyntectStyle, String)>>,
    pub line_selections: Vec<bool>,
}
