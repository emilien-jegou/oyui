//! Token-level (inline) diffing over syntax trees.
//!
//! The structural diff aligns subtrees, but when two subtrees differ the
//! interesting question is *what* changed inside them. This module refines a
//! changed byte span by diffing the tree's **leaf tokens** (atoms, delimiters)
//! over that span and returning the ranges of tokens that are novel on each
//! side.
//!
//! Because tokens are whole leaves, the refinement never splits an identifier,
//! a string literal or an operator in half, and whitespace between tokens is
//! never highlighted on its own.
//!
//! This is the engine behind "Myers + inline diff via tree-sitter" (line-level
//! alignment elsewhere, token-level highlights here) as well as the fine-grained
//! highlights of the structural line diff in [`crate::line_ops`].
//!
//! # Example
//!
//! ```
//! use oyui_syndiff::{build_tree, diff_inline};
//!
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
//!
//! let old = "let value = compute(foo, 1);";
//! let new = "let value = compute(bar, 2);";
//!
//! let old_tree = build_tree(parser.parse(old, None).unwrap().walk(), old);
//! # parser.reset();
//! let new_tree = build_tree(parser.parse(new, None).unwrap().walk(), new);
//!
//! let result = diff_inline(
//!     &old_tree,
//!     old,
//!     0..old.len(),
//!     &new_tree,
//!     new,
//!     0..new.len(),
//! );
//!
//! // `foo` and `1` became novel on the left, `bar` and `2` on the right.
//! assert_eq!(
//!     result.lhs_ranges.iter().map(|r| &old[r.clone()]).collect::<Vec<_>>(),
//!     vec!["foo", "1"]
//! );
//! assert_eq!(
//!     result.rhs_ranges.iter().map(|r| &new[r.clone()]).collect::<Vec<_>>(),
//!     vec!["bar", "2"]
//! );
//! ```

use std::ops::Range;

use imara_diff::{Algorithm, Diff, InternedInput, Interner};

use crate::syntax_tree::{SyntaxHint, SyntaxId, SyntaxTree};

/// The result of a token-level diff over a pair of byte spans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineDiff {
    /// Byte ranges of novel tokens on the left side (absolute offsets).
    ///
    /// Ranges may also include horizontal whitespace between adjacent changed
    /// tokens so they render as one coherent highlight.
    pub lhs_ranges: Vec<Range<usize>>,
    /// Byte ranges of novel tokens on the right side (absolute offsets),
    /// possibly including horizontal whitespace between adjacent changes.
    pub rhs_ranges: Vec<Range<usize>>,
}

/// Diffs the leaf tokens of two syntax trees restricted to byte spans.
///
/// Returns the byte ranges of tokens that only exist on one side, as absolute
/// offsets into the respective sources. Tokens are whole tree-sitter leaves
/// (atoms and delimiters); comment leaves are additionally split into words so
/// that editing a comment highlights only the words that changed.
///
/// Tokens overlapping a span boundary are clamped to it, which keeps mapping
/// to bounded regions (e.g. a single line of a multi-line token) simple.
///
/// Whitespace between tokens is ignored by construction: a change that only
/// re-spaces tokens yields empty ranges.
pub fn diff_inline(
    lhs_tree: &SyntaxTree,
    lhs_source: &str,
    lhs_span: Range<usize>,
    rhs_tree: &SyntaxTree,
    rhs_source: &str,
    rhs_span: Range<usize>,
) -> InlineDiff {
    let lhs_tokens = token_ranges(lhs_tree, lhs_source, lhs_span);
    let rhs_tokens = token_ranges(rhs_tree, rhs_source, rhs_span);

    let mut input = InternedInput {
        before: Vec::with_capacity(lhs_tokens.len()),
        after: Vec::with_capacity(rhs_tokens.len()),
        interner: Interner::new(lhs_tokens.len() + rhs_tokens.len()),
    };

    input.update_before(lhs_tokens.iter().map(|r| &lhs_source[r.clone()]));
    input.update_after(rhs_tokens.iter().map(|r| &rhs_source[r.clone()]));

    let diff = Diff::compute(Algorithm::Myers, &input);

    let mut lhs_ranges = Vec::new();
    let mut rhs_ranges = Vec::new();

    for hunk in diff.hunks() {
        for idx in hunk.before.clone() {
            lhs_ranges.push(lhs_tokens[idx as usize].clone());
        }
        for idx in hunk.after.clone() {
            rhs_ranges.push(rhs_tokens[idx as usize].clone());
        }
    }

    // Coalesce adjacent changed tokens across horizontal whitespace so a
    // phrase reads as one highlight; whitespace-only changes remain unmarked.
    InlineDiff {
        lhs_ranges: merge_changed_whitespace(lhs_source, merge_overlapping(lhs_ranges)),
        rhs_ranges: merge_changed_whitespace(rhs_source, merge_overlapping(rhs_ranges)),
    }
}

/// Diffs two source spans without requiring a language parser.
///
/// Word runs are kept whole; non-whitespace punctuation is tokenized
/// individually. This is intended as a useful fallback for prose and
/// unsupported languages, while [`diff_inline`] remains preferable when
/// syntax trees are available. Returned ranges are absolute byte offsets into
/// the respective source strings.
pub fn diff_inline_words(
    lhs_source: &str,
    lhs_span: Range<usize>,
    rhs_source: &str,
    rhs_span: Range<usize>,
) -> InlineDiff {
    let lhs_tokens = text_token_ranges(lhs_source, lhs_span);
    let rhs_tokens = text_token_ranges(rhs_source, rhs_span);

    let mut input = InternedInput {
        before: Vec::with_capacity(lhs_tokens.len()),
        after: Vec::with_capacity(rhs_tokens.len()),
        interner: Interner::new(lhs_tokens.len() + rhs_tokens.len()),
    };
    input.update_before(lhs_tokens.iter().map(|r| &lhs_source[r.clone()]));
    input.update_after(rhs_tokens.iter().map(|r| &rhs_source[r.clone()]));

    let diff = Diff::compute(Algorithm::Myers, &input);
    let mut lhs_ranges = Vec::new();
    let mut rhs_ranges = Vec::new();
    for hunk in diff.hunks() {
        lhs_ranges.extend(
            hunk.before
                .clone()
                .map(|idx| lhs_tokens[idx as usize].clone()),
        );
        rhs_ranges.extend(
            hunk.after
                .clone()
                .map(|idx| rhs_tokens[idx as usize].clone()),
        );
    }

    InlineDiff {
        lhs_ranges: merge_changed_whitespace(lhs_source, merge_overlapping(lhs_ranges)),
        rhs_ranges: merge_changed_whitespace(rhs_source, merge_overlapping(rhs_ranges)),
    }
}

fn text_token_ranges(source: &str, span: Range<usize>) -> Vec<Range<usize>> {
    let Some(text) = source.get(span.clone()) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut word_start = None;
    for (offset, ch) in text.char_indices() {
        let is_word = ch.is_alphanumeric() || ch == '_';
        if is_word {
            word_start.get_or_insert(offset);
        } else {
            if let Some(start) = word_start.take() {
                ranges.push(span.start + start..span.start + offset);
            }
            if !ch.is_whitespace() {
                ranges.push(span.start + offset..span.start + offset + ch.len_utf8());
            }
        }
    }
    if let Some(start) = word_start {
        ranges.push(span.start + start..span.end);
    }
    ranges
}

fn merge_changed_whitespace(source: &str, ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last)
                if range.start <= last.end
                    || source.get(last.end..range.start).is_some_and(|gap| {
                        !gap.is_empty() && gap.chars().all(|ch| ch == ' ' || ch == '\t')
                    }) =>
            {
                last.end = last.end.max(range.end);
            }
            _ => merged.push(range),
        }
    }
    merged
}

/// Collects the leaf token ranges of `tree` that intersect `span`.
///
/// Every node contributes at most its own non-trivia bytes: atoms contribute
/// their byte range, list nodes contribute their open/close delimiters and
/// recurse into their children. Ranges are clamped to `span`; empty results
/// are dropped.
fn token_ranges(tree: &SyntaxTree, source: &str, span: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    if let Some(root) = tree.root() {
        push_token_ranges(tree, root, source, &span, &mut out);
    }
    out
}

fn push_token_ranges(
    tree: &SyntaxTree,
    id: SyntaxId,
    source: &str,
    span: &Range<usize>,
    out: &mut Vec<Range<usize>>,
) {
    let node = tree.get(id);

    let push = |range: &Range<usize>, out: &mut Vec<Range<usize>>| {
        let start = range.start.max(span.start);
        let end = range.end.min(span.end);
        if start < end {
            out.push(start..end);
        }
    };

    if let Some(open) = node.open_delimiter_range() {
        push(&open, out);
    }

    // Delimiter-wrapped nodes (even empty ones like `()`) contribute their
    // delimiters and children individually; only bare atoms emit one token.
    if node.is_atom() && !node.has_delimiters() {
        if matches!(node.hint, Some(SyntaxHint::Comment(_))) {
            // Comments are free-form text: split them into words so that an
            // edited comment only highlights the words that changed.
            let start = node.byte_range.start.max(span.start);
            let end = node.byte_range.end.min(span.end);
            if let Some(text) = source.get(start..end) {
                for range in word_ranges(text) {
                    out.push(start + range.start..start + range.end);
                }
            }
        } else {
            push(&node.byte_range, out);
        }
    } else {
        let mut child = tree.first_child(id);
        while let Some(current) = child {
            push_token_ranges(tree, current, source, span, out);
            child = tree.next_sibling(current);
        }
    }

    if let Some(close) = node.close_delimiter_range() {
        push(&close, out);
    }
}

/// Byte ranges (relative to `text`) of the non-whitespace runs of `text`.
pub(crate) fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();

    let mut run_start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = run_start.take() {
                ranges.push(start..idx);
            }
        } else if run_start.is_none() {
            run_start = Some(idx);
        }
    }
    if let Some(start) = run_start {
        ranges.push(start..text.len());
    }

    ranges
}

/// Merges ranges that overlap or touch, preserving order.
///
/// Merges *byte-adjacent* ranges too, so tokens that ran together (e.g. two
/// adjacent novel tokens separated by no trivia) highlight as one span.
/// Inputs may be arbitrary; the result is sorted.
fn merge_overlapping(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return ranges;
    }

    ranges.sort_by_key(|r| r.start);
    let mut merged = vec![ranges[0].clone()];
    for range in ranges.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged is non-empty");
        if range.start <= last.end {
            // Imara yields equal tokens in matched position, so true overlaps
            // cannot occur in well-formed input; `max` is defensive.
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_tree;

    fn parse(source: &str) -> SyntaxTree<'_> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("rust language");
        let tree = parser.parse(source, None).expect("parse");
        build_tree(tree.walk(), source)
    }

    #[test]
    fn comment_words_are_split() {
        let text = "the quick brown fox";
        assert_eq!(
            word_ranges(text),
            vec![
                0..3,   // the
                4..9,   // quick
                10..15, // brown
                16..19, // fox
            ]
        );
        assert!(word_ranges("   \t ").is_empty());
    }

    #[test]
    fn whole_leaf_tokens_never_split() {
        // `countera` vs `counterb`: diffing the whole lines must highlight the
        // entire identifiers, not some suffixes thereof.
        let old = "let countera = 1;";
        let new = "let counterb = 1;";
        let (lhs, rhs) = (parse(old), parse(new));

        let diff = diff_inline(&lhs, old, 0..old.len(), &rhs, new, 0..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["countera"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["counterb"]
        );
    }

    #[test]
    fn whitespace_between_tokens_is_never_novel() {
        let old = "foo( a )";
        let new = "foo(a)";
        let (lhs, rhs) = (parse(old), parse(new));

        let diff = diff_inline(&lhs, old, 0..old.len(), &rhs, new, 0..new.len());

        assert!(diff.lhs_ranges.is_empty(), "{:?} of {old}", diff.lhs_ranges);
        assert!(diff.rhs_ranges.is_empty(), "{:?} of {new}", diff.rhs_ranges);
    }

    #[test]
    fn spans_sub_select_the_tokens() {
        let old = "alpha beta\ngamma delta";
        let new = "alpha BETTA\ngamma DELTA";
        let (lhs, rhs) = (parse(old), parse(new));

        // Only diff the second line (bytes 11..22 on both sides).
        let diff = diff_inline(&lhs, old, 11..old.len(), &rhs, new, 11..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["delta"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["DELTA"]
        );
    }

    #[test]
    fn comments_highlight_words_only() {
        let old = "// the quick brown fox\nfn a() {}";
        let new = "// the quick red fox\nfn a() {}";
        let (lhs, rhs) = (parse(old), parse(new));

        let diff = diff_inline(&lhs, old, 0..old.len(), &rhs, new, 0..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["brown"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["red"]
        );
    }

    #[test]
    fn parser_free_word_diff_highlights_changed_prose() {
        let old = "A prototype now selects stdout when it is a terminal";
        let new = "The event loop selects stdout when it is a terminal";

        let diff = diff_inline_words(old, 0..old.len(), new, 0..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["A prototype now"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["The event loop"]
        );
    }

    #[test]
    fn inline_highlights_join_changed_tokens_across_spaces() {
        let old = "inner: terminal_output()?";
        let new = "inner: Box<dyn Write>";
        let (lhs_tree, rhs_tree) = (parse(old), parse(new));
        let diff = diff_inline(&lhs_tree, old, 0..old.len(), &rhs_tree, new, 0..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["terminal_output()?"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["Box<dyn Write>"]
        );
    }

    #[test]
    fn delimiters_take_part_in_the_diff() {
        let old = "foo(x);";
        let new = "block(x);";
        let (lhs, rhs) = (parse(old), parse(new));

        let diff = diff_inline(&lhs, old, 0..old.len(), &rhs, new, 0..new.len());

        assert_eq!(
            diff.lhs_ranges
                .iter()
                .map(|r| &old[r.clone()])
                .collect::<Vec<_>>(),
            vec!["foo"]
        );
        assert_eq!(
            diff.rhs_ranges
                .iter()
                .map(|r| &new[r.clone()])
                .collect::<Vec<_>>(),
            vec!["block"]
        );
    }
}
