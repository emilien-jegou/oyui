use std::fmt;
use std::ops::Range;
use std::sync::Arc;

pub mod conflict;
pub mod line_selections;
pub mod merge3;
pub mod staging;

pub use conflict::{ConflictedFile, Side};
pub use line_selections::LineSelections;

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

#[derive(Clone)]
pub struct FileDiff {
    pub old_file_content: Arc<str>,
    pub new_file_content: Arc<str>,
    pub hunks: Vec<Hunk>,

    /// Tracks which hunks/lines are staged/selected by the user
    pub line_selections: LineSelections,
}

impl fmt::Debug for FileDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileDiff").finish()
    }
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

/// Byte range of every line of one side of a diff, in line order.
///
/// Matches `str::lines()` exactly, including stripping a trailing `\r` and
/// yielding no final empty line after a trailing newline.
pub type LineRanges = Vec<Range<usize>>;

/// Builds the line ranges of `content`.
///
/// The diff already walks both sides to build its line index; publishing the
/// result lets the renderer index a line in constant time instead of rescanning
/// the whole file on every frame.
pub fn line_ranges(content: &str) -> LineRanges {
    let bytes = content.as_bytes();
    let mut ranges = Vec::with_capacity(bytes.len() / 24 + 1);
    let mut start = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b != b'\n' {
            continue;
        }
        let mut end = i;
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        ranges.push(start..end);
        start = i + 1;
    }
    if start < bytes.len() {
        ranges.push(start..bytes.len());
    }
    ranges
}

/// Borrowed view of one side's lines, indexed without rescanning the content.
///
/// The renderer formerly collected `content.lines()` into a `Vec<&str>` for both
/// sides of the file on every frame; this replaces that with a read of the
/// precomputed ranges, so a frame costs a lookup rather than a full pass.
pub struct LineAccess<'a> {
    content: &'a str,
    ranges: &'a [Range<usize>],
}

impl<'a> LineAccess<'a> {
    /// Borrows `content` and its precomputed `ranges`.
    pub fn new(content: &'a str, ranges: &'a [Range<usize>]) -> Self {
        Self { content, ranges }
    }

    /// The line at `idx`, or `None` when out of range.
    pub fn get(&self, idx: usize) -> Option<&'a str> {
        let range = self.ranges.get(idx)?;
        self.content.get(range.clone())
    }

    /// Number of lines.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// True when this side has no lines.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// The line at `idx`, or an empty slice when out of range.
    ///
    /// Borrowed for `'a` rather than for `&self`, because the content is.
    pub fn line(&self, idx: usize) -> &'a str {
        self.get(idx).unwrap_or_default()
    }
}
#[cfg(test)]
mod line_access_tests {
    use super::{line_ranges, LineAccess};

    /// The computed ranges must agree with `str::lines()` line for line, or
    /// every rendered line would be offset by the difference.
    #[test]
    fn line_ranges_match_str_lines() {
        for content in [
            "",
            "a",
            "a\n",
            "a\nb",
            "a\nb\n",
            "a\r\nb\r\n",
            "\n\n\n",
            "no trailing newline",
            "tabs\tinside\n----\n",
            "unicode \u{2713} \u{6f22}\u{5b57}\nsecond",
        ] {
            let ranges = line_ranges(content);
            let expected: Vec<&str> = content.lines().collect();

            assert_eq!(
                ranges.len(),
                expected.len(),
                "line count for {content:?}: {ranges:?}"
            );
            for (i, line) in expected.iter().enumerate() {
                assert_eq!(
                    &content[ranges[i].clone()],
                    *line,
                    "line {i} of {content:?}"
                );
            }
        }
    }

    #[test]
    fn line_access_indexes_and_clamps() {
        let content = "one\ntwo\nthree";
        let ranges = line_ranges(content);
        let access = LineAccess::new(content, &ranges);

        assert_eq!(access.len(), 3);
        assert_eq!(access.line(0), "one");
        assert_eq!(access.get(2), Some("three"));
        assert_eq!(access.get(3), None);
        assert_eq!(access.line(9), "");
        assert!(!access.is_empty());
        assert!(LineAccess::new("", &[]).is_empty());
    }
}
