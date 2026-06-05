use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

use crate::cli::DiffAlgorithm;
use crate::commons::lazy::Lazy;
use crate::diff::{DiffLine, DiffResult, FileDiff, Hunk, InlineChange};
use crate::diff_cache::DiffCache;

const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB limit

pub struct FullDiff;

#[derive(Debug, Clone)]
pub struct FullDiffReq {
    pub node_path: PathBuf,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FullDiffRes {
    pub node_path: PathBuf,
    pub result: DiffResult,
    pub right_path: Option<PathBuf>,
}

async fn load_text_safely(path: &PathBuf) -> Result<String, DiffResult> {
    let meta = match fs::metadata(path).await {
        Ok(m) => m,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let size = meta.len();

    if size > MAX_FILE_SIZE {
        return Err(DiffResult::TooLarge(size));
    }

    let buffer = match fs::read(path).await {
        Ok(b) => b,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let get_binary_info = |buf: &[u8]| -> DiffResult {
        let kind = infer::get(buf);
        let mime = kind
            .map(|k| k.mime_type().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let ext = kind
            .map(|k| k.extension().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        DiffResult::Binary { size, mime, ext }
    };

    let check_len = std::cmp::min(buffer.len(), 8000);
    if buffer[..check_len].contains(&0) {
        return Err(get_binary_info(&buffer));
    }

    match String::from_utf8(buffer) {
        Ok(text) => Ok(text),
        Err(e) => {
            let original_buffer = e.into_bytes();
            Err(get_binary_info(&original_buffer))
        }
    }
}

struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        Self { text, starts }
    }

    fn printable_byte_range(&self, line_idx: usize) -> Range<usize> {
        let start = self.starts.get(line_idx).copied().unwrap_or(0);
        let mut end = self
            .starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.text.len());

        let bytes = self.text.as_bytes();
        while end > start {
            let b = bytes[end - 1];
            if b == b'\n' || b == b'\r' {
                end -= 1;
            } else {
                break;
            }
        }
        start..end
    }
}

/// Merges overlapping or adjacent `InlineChange` byte ranges.
fn merge_inline_changes(mut changes: Vec<InlineChange>) -> Vec<InlineChange> {
    changes.sort_by_key(|c| c.byte_range.start);
    let mut merged: Vec<InlineChange> = Vec::new();
    for c in changes {
        if let Some(last) = merged.last_mut() {
            if c.byte_range.start <= last.byte_range.end {
                last.byte_range.end = last.byte_range.end.max(c.byte_range.end);
                continue;
            }
        }
        merged.push(c);
    }
    merged
}

/// Whether `c` is treated as a word character.
/// Unlike `is_alphanumeric`, underscore is NOT a word char so that
/// `snake_case` identifiers split into separate tokens.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c > '\x7F'
}

/// Tokenize a byte range into line tokens (each line ending with `\n`).
fn tokenize_lines(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut line_start = range.start;
    for (i, ch) in text[range.clone()].char_indices() {
        let pos = range.start + i;
        if ch == '\n' {
            ranges.push(line_start..pos + 1);
            line_start = pos + 1;
        }
    }
    if line_start < range.end {
        ranges.push(line_start..range.end);
    }
    ranges
}

/// Tokenize a byte range into word tokens (runs of word characters).
/// Non-word characters are skipped.
fn tokenize_words(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut word_start: Option<usize> = None;
    for (i, ch) in text[range.clone()].char_indices() {
        let pos = range.start + i;
        if is_word_char(ch) {
            if word_start.is_none() {
                word_start = Some(pos);
            }
        } else if let Some(start) = word_start.take() {
            ranges.push(start..pos);
        }
    }
    if let Some(start) = word_start {
        ranges.push(start..range.end);
    }
    ranges
}

/// Tokenize a byte range into individual non-word characters.
/// Word characters are skipped — they are handled at the word level.
fn tokenize_nonwords(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for (i, ch) in text[range.clone()].char_indices() {
        let pos = range.start + i;
        if !is_word_char(ch) {
            ranges.push(pos..pos + ch.len_utf8());
        }
    }
    ranges
}

/// A token source backed by a `&str` and a pre-computed range list.
struct StrTokenSource<'a> {
    text: &'a str,
    ranges: Vec<Range<usize>>,
}

impl<'a> imara_diff::TokenSource for StrTokenSource<'a> {
    type Token = String;
    type Tokenizer = Box<dyn Iterator<Item = String> + 'a>;

    fn tokenize(&self) -> Self::Tokenizer {
        let text = self.text;
        let ranges = self.ranges.clone();
        Box::new(ranges.into_iter().map(move |r| text[r].to_string()))
    }

    fn estimate_tokens(&self) -> u32 {
        self.ranges.len() as u32
    }
}

/// Run a single-level histogram diff of `old[left]` vs `new[right]`.
///
/// Returns matching `(old_range, new_range)` pairs.
fn single_level_diff(
    old: &str,
    left: Range<usize>,
    new: &str,
    right: Range<usize>,
    tokenizer: impl Fn(&str, Range<usize>) -> Vec<Range<usize>>,
) -> Vec<(Range<usize>, Range<usize>)> {
    let left_ranges = tokenizer(old, left);
    let right_ranges = tokenizer(new, right);

    if left_ranges.is_empty() || right_ranges.is_empty() {
        return vec![];
    }

    let left_source = StrTokenSource { text: old, ranges: left_ranges.clone() };
    let right_source = StrTokenSource { text: new, ranges: right_ranges.clone() };

    let input = InternedInput::new(left_source, right_source);
    let diff = Diff::compute(Algorithm::Histogram, &input);

    let mut matches: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    let mut li = 0usize;
    let mut ri = 0usize;

    for hunk in diff.hunks() {
        let l_end = hunk.before.end as usize;
        let r_end = hunk.after.end as usize;
        let l_start = hunk.before.start as usize;
        let r_start = hunk.after.start as usize;

        while li < l_start && ri < r_start {
            matches.push((left_ranges[li].clone(), right_ranges[ri].clone()));
            li += 1;
            ri += 1;
        }

        li = l_end;
        ri = r_end;
    }

    while li < left_ranges.len() && ri < right_ranges.len() {
        matches.push((left_ranges[li].clone(), right_ranges[ri].clone()));
        li += 1;
        ri += 1;
    }

    matches
}

/// Refine existing matches by re-diffing each gap at a finer granularity.
/// Returns a new match list with sub-matches inserted.
fn refine_matches(
    old: &str,
    new: &str,
    matches: &[(Range<usize>, Range<usize>)],
    tokenizer: impl Fn(&str, Range<usize>) -> Vec<Range<usize>>,
) -> Vec<(Range<usize>, Range<usize>)> {
    let mut refined = Vec::with_capacity(matches.len());
    refined.push(matches[0].clone());

    for window in matches.windows(2) {
        let (prev_left, prev_right) = &window[0];
        let (next_left, next_right) = &window[1];

        if prev_left.end < next_left.start && prev_right.end < next_right.start {
            let gap_left = prev_left.end..next_left.start;
            let gap_right = prev_right.end..next_right.start;
            refined.extend(single_level_diff(old, gap_left, new, gap_right, &tokenizer));
        }

        refined.push((next_left.clone(), next_right.clone()));
    }

    refined
}

/// Computes multi-level (line → word → nonword) differences between two
/// strings using jj-style histogram refinement.
///
/// Returns `(old_ranges, new_ranges)` where each range is a UTF-8-safe byte
/// range within the respective string that differs between the old and new
/// text.
fn char_diff(old: &str, new: &str) -> (Vec<InlineChange>, Vec<InlineChange>) {
    if old.is_empty() || new.is_empty() || old == new {
        return (Vec::new(), Vec::new());
    }

    let len_old = old.len();
    let len_new = new.len();

    // Level 1: line-level diff
    let raw = single_level_diff(old, 0..len_old, new, 0..len_new, tokenize_lines);

    // Build match list with empty boundary sentinels
    let mut matches: Vec<(Range<usize>, Range<usize>)> = Vec::with_capacity(raw.len() + 2);
    matches.push((0..0, 0..0));
    matches.extend(raw);
    matches.push((len_old..len_old, len_new..len_new));

    // Level 2: word-level refinement
    matches = refine_matches(old, new, &matches, tokenize_words);

    // Level 3: nonword-level refinement
    matches = refine_matches(old, new, &matches, tokenize_nonwords);

    // Convert gaps between final matches to removed/added ranges
    let mut old_ranges = Vec::new();
    let mut new_ranges = Vec::new();

    for window in matches.windows(2) {
        let (prev_left, prev_right) = &window[0];
        let (next_left, next_right) = &window[1];

        if prev_left.end < next_left.start {
            old_ranges.push(InlineChange { byte_range: prev_left.end..next_left.start });
        }
        if prev_right.end < next_right.start {
            new_ranges.push(InlineChange { byte_range: prev_right.end..next_right.start });
        }
    }

    (
        merge_inline_changes(old_ranges),
        merge_inline_changes(new_ranges),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_diff_identical() {
        let (old, new) = char_diff("hello", "hello");
        assert!(old.is_empty());
        assert!(new.is_empty());
    }

    #[test]
    fn char_diff_empty() {
        let (old, new) = char_diff("", "hello");
        assert!(old.is_empty());
        assert!(new.is_empty());

        let (old, new) = char_diff("hello", "");
        assert!(old.is_empty());
        assert!(new.is_empty());

        let (old, new) = char_diff("", "");
        assert!(old.is_empty());
        assert!(new.is_empty());
    }

    #[test]
    fn char_diff_completely_different() {
        let (old, new) = char_diff("abc", "xyz");
        assert_eq!(old, vec![InlineChange { byte_range: 0..3 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..3 }]);
    }

    #[test]
    fn char_diff_partial_change() {
        // Both are single word tokens → word-level mismatch, no nonword chars
        // to refine → full word highlighted
        let (old, new) = char_diff("foobar", "foobaz");
        assert_eq!(old, vec![InlineChange { byte_range: 0..6 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..6 }]);
    }

    #[test]
    fn char_diff_insertion() {
        let (old, new) = char_diff("abc", "abcxyz");
        assert_eq!(old, vec![InlineChange { byte_range: 0..3 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..6 }]);
    }

    #[test]
    fn char_diff_deletion() {
        let (old, new) = char_diff("abcxyz", "abc");
        assert_eq!(old, vec![InlineChange { byte_range: 0..6 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..3 }]);
    }

    #[test]
    fn char_diff_prepend() {
        let (old, new) = char_diff("abc", "xyzabc");
        assert_eq!(old, vec![InlineChange { byte_range: 0..3 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..6 }]);
    }

    #[test]
    fn char_diff_unicode() {
        // All chars are word chars (alphanumeric or non-ASCII) → no nonword refinement
        let (old, new) = char_diff("café", "cafè");
        assert_eq!(old, vec![InlineChange { byte_range: 0..5 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..5 }]);
    }

    #[test]
    fn char_diff_single_char() {
        let (old, new) = char_diff("a", "b");
        assert_eq!(old, vec![InlineChange { byte_range: 0..1 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 0..1 }]);
    }

    #[test]
    fn char_diff_whitespace_indent() {
        // "  foo" → "    foo": "foo" matches at word level.
        // Nonword level matches individual spaces: 2 common, 2 extra at end.
        let (old, new) = char_diff("  foo", "    foo");
        assert!(old.is_empty());
        assert_eq!(new, vec![InlineChange { byte_range: 2..4 }]);
    }

    #[test]
    fn char_diff_whitespace_dedent() {
        let (old, new) = char_diff("    foo", "  foo");
        assert_eq!(old, vec![InlineChange { byte_range: 2..4 }]);
        assert!(new.is_empty());
    }

    #[test]
    fn char_diff_multi_word() {
        // "abc def ghi" → "abc xyz ghi": "abc" and "ghi" match at word level.
        // Spaces around middle word match at nonword level.
        // Only "def" vs "xyz" remains.
        let (old, new) = char_diff("abc def ghi", "abc xyz ghi");
        assert_eq!(old, vec![InlineChange { byte_range: 4..7 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 4..7 }]);
    }

    #[test]
    fn char_diff_underscore_word() {
        // Underscore breaks words, so foo_bar → foo_baz highlights only
        // "bar" vs "baz", not the whole identifier.
        let (old, new) = char_diff("fn foo_bar() {}", "fn foo_baz() {}");
        assert_eq!(old, vec![InlineChange { byte_range: 7..10 }]);
        assert_eq!(new, vec![InlineChange { byte_range: 7..10 }]);
    }

    #[test]
    fn char_diff_leading_newline_skip() {
        // old: "\n    foo" (newline + 4 spaces + foo)
        // new: "\n  foo"   (newline + 2 spaces + foo)
        // "foo" matches at word level. Newline and common spaces match
        // at nonword level. Only the trailing 2 extra spaces highlight.
        let (old, new) = char_diff("\n    foo", "\n  foo");
        assert_eq!(old, vec![InlineChange { byte_range: 3..5 }]);
        assert!(new.is_empty());
    }

    #[test]
    fn tokenize_lines_basic() {
        let tokens = tokenize_lines("abc\ndef\n", 0..8);
        assert_eq!(tokens, vec![0..4, 4..8]);
    }

    #[test]
    fn tokenize_words_basic() {
        let tokens = tokenize_words("foo bar", 0..7);
        assert_eq!(tokens, vec![0..3, 4..7]);
    }

    #[test]
    fn tokenize_words_underscore() {
        // Underscore is not a word char → splits the identifier
        let tokens = tokenize_words("foo_bar", 0..7);
        assert_eq!(tokens, vec![0..3, 4..7]);
    }

    #[test]
    fn tokenize_nonwords_basic() {
        let tokens = tokenize_nonwords("a b", 0..3);
        assert_eq!(tokens, vec![1..2]);
    }

    #[test]
    fn tokenize_nonwords_skips_word_chars() {
        let tokens = tokenize_nonwords("abc", 0..3);
        assert!(tokens.is_empty());
    }
}

pub fn compute(
    algo: &DiffAlgorithm,
    left_text: &str,
    right_text: &str,
    path: &Path,
    inline_diff: bool,
) -> Result<Vec<Hunk>, Box<dyn std::error::Error + Send + Sync>> {
    let input = InternedInput::new(left_text, right_text);

    let inner_algo = match algo {
        DiffAlgorithm::Histogram | DiffAlgorithm::SyntaxAware => Algorithm::Histogram,
        DiffAlgorithm::Myers => Algorithm::Myers,
        DiffAlgorithm::MyersMinimal => Algorithm::MyersMinimal,
    };

    let diff = Diff::compute(inner_algo, &input);

    let old_idx = LineIndex::new(left_text);
    let new_idx = LineIndex::new(right_text);

    let syntax_res = if *algo == DiffAlgorithm::SyntaxAware {
        match oyui_syndiff::diff_source(left_text, right_text, path, None) {
            Ok(res) => Some(res),
            Err(e) => {
                tracing::debug!(
                    "Syntax diff unavailable/failed ({}). Falling back to text.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let mut hunks = Vec::new();

    for hunk in diff.hunks() {
        let mut lines = Vec::new();

        let get_highlights = |line_range: Range<usize>,
                              struct_ranges: &[Range<usize>],
                              text: &str|
         -> Vec<InlineChange> {
            let line_text = &text[line_range.start..line_range.end];
            let trimmed_len = line_text.trim().len();

            if trimmed_len == 0 {
                return Vec::new();
            }

            if let Some(_syntax) = &syntax_res {
                let mut raw_ranges = Vec::new();

                for r in struct_ranges {
                    if r.start < line_range.end && r.end > line_range.start {
                        let clamp_start = r.start.max(line_range.start) - line_range.start;
                        let clamp_end = r.end.min(line_range.end) - line_range.start;
                        raw_ranges.push(clamp_start..clamp_end);
                    }
                }

                if raw_ranges.is_empty() {
                    return Vec::new();
                }

                raw_ranges.sort_by_key(|r| r.start);
                let mut merged = vec![raw_ranges[0].clone()];
                for r in raw_ranges.into_iter().skip(1) {
                    let last = merged.last_mut().unwrap();
                    if r.start <= last.end {
                        last.end = last.end.max(r.end);
                    } else {
                        merged.push(r);
                    }
                }

                let total_highlighted: usize = merged.iter().map(|r| r.end - r.start).sum();

                if total_highlighted >= trimmed_len {
                    return Vec::new();
                }

                return merged
                    .into_iter()
                    .map(|byte_range| InlineChange { byte_range })
                    .collect();
            }

            Vec::new()
        };

        for i in (hunk.before.start as usize)..(hunk.before.end as usize) {
            let line_range = old_idx.printable_byte_range(i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.old_ranges.as_slice())
                    .unwrap_or(&[]),
                left_text,
            );
            lines.push(DiffLine::Deletion {
                old_line_idx: i,
                inline_highlights,
            });
        }

        for i in (hunk.after.start as usize)..(hunk.after.end as usize) {
            let line_range = new_idx.printable_byte_range(i);
            let inline_highlights = get_highlights(
                line_range,
                syntax_res
                    .as_ref()
                    .map(|s| s.new_ranges.as_slice())
                    .unwrap_or(&[]),
                right_text,
            );
            lines.push(DiffLine::Addition {
                new_line_idx: i,
                inline_highlights,
            });
        }

        // Section-based block-diff: find consecutive -/+ groups and run
        // char_diff on concatenated section text.
        if inline_diff {
            let mut si = 0;
        while si < lines.len() {
            while si < lines.len() && !matches!(lines[si], DiffLine::Deletion { .. }) {
                si += 1;
            }
            if si >= lines.len() {
                break;
            }
            let del_start = si;
            let mut del_count = 0;
            while si < lines.len() && matches!(lines[si], DiffLine::Deletion { .. }) {
                del_count += 1;
                si += 1;
            }

            let add_start = si;
            let mut add_count = 0;
            while si < lines.len() && matches!(lines[si], DiffLine::Addition { .. }) {
                add_count += 1;
                si += 1;
            }

            if del_count == 0 || add_count == 0 {
                continue;
            }

            // Build concatenated old text and record per-line byte offsets
            let mut old_concat = String::new();
            let mut old_offsets: Vec<Range<usize>> = Vec::with_capacity(del_count);
            for j in 0..del_count {
                let line_idx = match &lines[del_start + j] {
                    DiffLine::Deletion { old_line_idx, .. } => *old_line_idx,
                    _ => unreachable!(),
                };
                let text = &left_text[old_idx.printable_byte_range(line_idx)];
                if j > 0 {
                    old_concat.push('\n');
                }
                let start = old_concat.len();
                old_concat.push_str(text);
                old_offsets.push(start..old_concat.len());
            }

            // Build concatenated new text and record per-line byte offsets
            let mut new_concat = String::new();
            let mut new_offsets: Vec<Range<usize>> = Vec::with_capacity(add_count);
            for j in 0..add_count {
                let line_idx = match &lines[add_start + j] {
                    DiffLine::Addition { new_line_idx, .. } => *new_line_idx,
                    _ => unreachable!(),
                };
                let text = &right_text[new_idx.printable_byte_range(line_idx)];
                if j > 0 {
                    new_concat.push('\n');
                }
                let start = new_concat.len();
                new_concat.push_str(text);
                new_offsets.push(start..new_concat.len());
            }

            // Run char_diff on concatenated section texts
            let (old_changes, new_changes) = char_diff(&old_concat, &new_concat);

            // Distribute old ranges to individual deletion lines
            let mut old_pending: Vec<Vec<InlineChange>> = (0..del_count).map(|_| Vec::new()).collect();
            for change in &old_changes {
                let r = &change.byte_range;
                for (li, lo) in old_offsets.iter().enumerate() {
                    let os = r.start.max(lo.start);
                    let oe = r.end.min(lo.end);
                    if os < oe {
                        old_pending[li].push(InlineChange { byte_range: (os - lo.start)..(oe - lo.start) });
                    }
                }
            }
            for (li, ranges) in old_pending.iter_mut().enumerate() {
                if ranges.is_empty() {
                    continue;
                }
                if let DiffLine::Deletion {
                    ref mut inline_highlights,
                    ..
                } = &mut lines[del_start + li]
                {
                    if inline_highlights.is_empty() {
                        *inline_highlights = merge_inline_changes(std::mem::take(ranges));
                    }
                }
            }

            // Distribute new ranges to individual addition lines
            let mut new_pending: Vec<Vec<InlineChange>> = (0..add_count).map(|_| Vec::new()).collect();
            for change in &new_changes {
                let r = &change.byte_range;
                for (li, lo) in new_offsets.iter().enumerate() {
                    let os = r.start.max(lo.start);
                    let oe = r.end.min(lo.end);
                    if os < oe {
                        new_pending[li].push(InlineChange { byte_range: (os - lo.start)..(oe - lo.start) });
                    }
                }
            }
            for (li, ranges) in new_pending.iter_mut().enumerate() {
                if ranges.is_empty() {
                    continue;
                }
                if let DiffLine::Addition {
                    ref mut inline_highlights,
                    ..
                } = &mut lines[add_start + li]
                {
                    if inline_highlights.is_empty() {
                        *inline_highlights = merge_inline_changes(std::mem::take(ranges));
                    }
                }
            }

            // Merge per-line highlights
            for i in del_start..(del_start + del_count) {
                if let DiffLine::Deletion {
                    ref mut inline_highlights,
                    ..
                } = &mut lines[i]
                {
                    if !inline_highlights.is_empty() {
                        *inline_highlights = merge_inline_changes(std::mem::take(inline_highlights));
                    }
                }
            }
            for i in add_start..(add_start + add_count) {
                if let DiffLine::Addition {
                    ref mut inline_highlights,
                    ..
                } = &mut lines[i]
                {
                    if !inline_highlights.is_empty() {
                        *inline_highlights = merge_inline_changes(std::mem::take(inline_highlights));
                    }
                }
            }
            }
        }

        hunks.push(Hunk {
            before_lines: (hunk.before.start as usize)..(hunk.before.end as usize),
            after_lines: (hunk.after.start as usize)..(hunk.after.end as usize),
            lines,
            marker: Default::default()
        });
    }

    Ok(hunks)
}

#[derive(TaskerContext)]
pub struct FullDiffContext {
    pub algorithm: DiffAlgorithm,
    pub inline_diff: Arc<RwLock<bool>>,
}

impl Listener<FullDiffReq, crate::worker::EventSender> for FullDiff {
    type Context = FullDiffContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(
        event: FullDiffReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!(
            left_path = ?event.left_path,
            right_path = ?event.right_path,
            "Computing full diff"
        );

        let left_path_clone = event.left_path.clone();
        let right_path_clone = event.right_path.clone();

        let left_fut = async {
            if let Some(p) = &left_path_clone {
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let right_fut = async {
            if let Some(p) = &right_path_clone {
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };
        let (left_res, right_res) = tokio::join!(left_fut, right_fut);

        let diff_result = match (left_res, right_res) {
            (Err(e), _) | (_, Err(e)) => e,
            (Ok(left_text), Ok(right_text)) => {
                if left_text.is_empty() && right_text.is_empty() {
                    tx.send(FullDiffRes {
                        node_path: event.node_path.clone(),
                        result: DiffResult::Empty,
                        right_path: event.right_path.clone(),
                    })?;
                    return Ok(());
                }

                match compute(&ctx.algorithm, &left_text, &right_text, &event.node_path, *ctx.inline_diff.read()) {
                    Ok(hunks) => DiffResult::Text(FileDiff {
                        old_text: Arc::from(left_text),
                        new_text: Arc::from(right_text),
                        hunks,
                        line_selections: Default::default(),
                    }),
                    Err(e) => DiffResult::Error(e.to_string()),
                }
            }
        };

        tracing::trace!("Full diff computation finished");
        tx.send(FullDiffRes {
            node_path: event.node_path.clone(),
            result: diff_result,
            right_path: event.right_path.clone(),
        })?;
        Ok(())
    }
}

#[derive(TaskerContext)]
pub struct FullDiffResCtx {
    pub cache: Arc<RwLock<DiffCache>>,
    pub syntax_theme: Arc<RwLock<Lazy<Arc<syntect::highlighting::Theme>>>>,
}

pub struct FullDiffResListener;
impl Listener<FullDiffRes, crate::worker::EventSender> for FullDiffResListener {
    type Context = FullDiffResCtx;

    async fn handle(
        mut event: FullDiffRes,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!(node_path = %event.node_path.display(), "Applied FullDiff cache");

        let mut cache = ctx.cache.write();

        // Preserve line_selections (staging state) from the previous cached diff,
        // so re-queueing doesn't lose user staging choices.
        let old_selections = cache.diffs.get(&event.node_path).value().and_then(|d| {
            if let crate::diff::DiffResult::Text(fd) = d {
                Some(fd.line_selections.clone())
            } else {
                None
            }
        });

        if let crate::diff::DiffResult::Text(ref file_diff) = event.result {
            let text = file_diff.new_text.clone();
            cache.syntax.mark_started(event.node_path.clone());

            tracing::trace!(node_path = %event.node_path.display(), "Queueing Syntax task");
            if let Some(val) = ctx.syntax_theme.read().value() {
                let _ = tx.send(crate::worker::tasks::syntax::SyntaxReq {
                    node_path: event.node_path.clone(),
                    text,
                    right_path: event.right_path.clone(),
                    theme: val.clone(),
                });
            }
        }

        if let crate::diff::DiffResult::Text(ref mut new_diff) = event.result {
            if let Some(selections) = old_selections {
                new_diff.line_selections = selections;
            }
        }

        cache.diffs.set(event.node_path, event.result);
        Ok(())
    }
}
