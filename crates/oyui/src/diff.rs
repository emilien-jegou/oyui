use std::ops::Range;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffStats {
    Text { insertions: usize, deletions: usize },
    Binary { bytes: isize },
}

/// Represents an exact byte-level change within a specific line.
///
/// The range is **relative to the line's starting byte**.
/// This makes it trivial for the TUI to slice the line string
/// into `ratatui` Spans without doing absolute offset math.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineChange {
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffLine {
    /// An unchanged context line
    Context {
        old_line_idx: usize,
        new_line_idx: usize,
    },
    /// A line deleted from the old text
    Deletion {
        old_line_idx: usize,
        /// Structural highlights (AST nodes) within this deleted line.
        /// Empty means the whole line was cleanly deleted.
        inline_highlights: Vec<InlineChange>,
    },
    /// A line inserted into the new text
    Addition {
        new_line_idx: usize,
        /// Structural highlights (AST nodes) within this inserted line.
        /// Empty means the whole line was cleanly added.
        inline_highlights: Vec<InlineChange>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HunkMarker {
    #[default]
    None,
    LineToggle,
    HunkSplit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Absolute line bounds in the old text (used for chunking / header display)
    pub before_lines: Range<usize>,
    /// Absolute line bounds in the new text (used for chunking / header display)
    pub after_lines: Range<usize>,

    /// The pre-computed, ordered sequence of lines to display in this hunk.
    /// This saves the TUI thread from doing string/line matching at render time.
    pub lines: Vec<DiffLine>,

    /// The type of marker for this hunk (e.g., line toggle, hunk split, or none).
    pub marker: HunkMarker,
}

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub old_file_content: Arc<str>,
    pub new_file_content: Arc<str>,
    pub hunks: Vec<Hunk>,

    /// Tracks which hunks/lines are staged/selected by the user
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
