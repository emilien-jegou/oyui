//! End-to-end tests for the tree-driven line diff ([`LineDiff`]) and the
//! `diff_source_lines` entry point, against the fixture corpus plus focused
//! cases.
//!
//! Note: `line_spans_of`/`Side::Lhs` helpers here mirror the crate's line
//! model for test-side range reconstruction; the crate functions are private.

use std::ops::Range;
use std::path::Path;

use common::{fixtures, get_parser};
use oyui_syndiff::{diff_line_ops, diff_source_lines, FailedDiff, LineDiff, LineOp};

mod common;

/// The novel lines of a line diff, per side: `Deleted`/`Added` rows are
/// wholly novel, `Replaced` rows carry novel inline bytes. (Line-level
/// novelty is per line; byte-level novelty is per token — the fixture
/// markers bridge the two by intersecting with line spans.)
fn novel_lines_of(diff: &LineDiff, side: Side) -> Vec<usize> {
    diff.ops
        .iter()
        .filter_map(|op| match (side, op) {
            (Side::Lhs, LineOp::Deleted { lhs }) => Some(*lhs),
            (
                Side::Lhs,
                LineOp::Replaced {
                    lhs, lhs_inline, ..
                },
            ) if !lhs_inline.is_empty() => Some(*lhs),
            (Side::Rhs, LineOp::Added { rhs }) => Some(*rhs),
            (
                Side::Rhs,
                LineOp::Replaced {
                    rhs, rhs_inline, ..
                },
            ) if !rhs_inline.is_empty() => Some(*rhs),
            _ => None,
        })
        .collect()
}

/// The novel lines according to byte ranges (the fixture markers): lines
/// intersecting any marked range.
fn novel_lines_from_ranges(ranges: &[Range<usize>], text: &str) -> Vec<usize> {
    let spans = line_spans_of(text);

    let mut marked = vec![false; spans.len()];
    for range in ranges {
        for (i, span) in spans.iter().enumerate() {
            if range.start < span.end && range.end > span.start {
                marked[i] = true;
            }
        }
    }

    (0..spans.len()).filter(|i| marked[*i]).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Lhs,
    Rhs,
}

fn line_spans_of(content: &str) -> Vec<Range<usize>> {
    content
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some(start..start + line.len())
        })
        .collect()
}

fn tree_of<'t>(parser: common::TreeParser, text: &'t str) -> oyui_syndiff::SyntaxTree<'t> {
    parser(text)
}

#[test]
fn fixture_line_ops_match_marked_novel_ranges() {
    for fx in fixtures() {
        let parser = get_parser(&fx.lang);
        let (lhs_tree, rhs_tree) = (tree_of(parser, &fx.lhs), tree_of(parser, &fx.rhs));

        let diff = diff_line_ops(&lhs_tree, &fx.lhs, &rhs_tree, &fx.rhs, None)
            .unwrap_or_else(|| panic!("{}: diff/graph limit", fx.path.display()));

        // Novel line sets must agree with the byte-level fixtures.
        assert_eq!(
            novel_lines_of(&diff, Side::Lhs),
            novel_lines_from_ranges(&fx.lhs_novel, &fx.lhs),
            "LHS novel lines mismatch for {}",
            fx.path.display()
        );
        assert_eq!(
            novel_lines_of(&diff, Side::Rhs),
            novel_lines_from_ranges(&fx.rhs_novel, &fx.rhs),
            "RHS novel lines mismatch for {}",
            fx.path.display()
        );

        // Inline ranges on Replaced rows equal the markers clipped to lines.
        check_replaced_inline(&diff, &fx.lhs, &fx.lhs_novel, Side::Lhs, &fx.path);
        check_replaced_inline(&diff, &fx.rhs, &fx.rhs_novel, Side::Rhs, &fx.path);

        assert_invariant(&diff, &fx.lhs, &fx.rhs, &fx.path);
    }
}

/// On `Replaced` rows the inline ranges must exactly equal the fixture
/// markers clipped to that line.
fn check_replaced_inline(
    diff: &LineDiff,
    text: &str,
    marked: &[Range<usize>],
    side: Side,
    path: &Path,
) {
    let spans = line_spans_of(text);

    for op in &diff.ops {
        let (line_idx, inline) = match (side, op) {
            (
                Side::Lhs,
                LineOp::Replaced {
                    lhs, lhs_inline, ..
                },
            ) => (*lhs, lhs_inline),
            (
                Side::Rhs,
                LineOp::Replaced {
                    rhs, rhs_inline, ..
                },
            ) => (*rhs, rhs_inline),
            _ => continue,
        };

        let line = spans[line_idx].clone();

        let expected: Vec<Range<usize>> = marked
            .iter()
            .filter_map(|r| {
                let start = r.start.max(line.start);
                let end = r.end.min(line.end);
                (start < end).then_some((start - line.start)..(end - line.start))
            })
            .collect();

        assert_eq!(
            inline,
            &expected,
            "{}: inline mismatch on {} line {line_idx}",
            path.display(),
            match side {
                Side::Lhs => "lhs",
                Side::Rhs => "rhs",
            },
        );
    }
}

fn assert_invariant(diff: &LineDiff, lhs: &str, rhs: &str, path: &Path) {
    let lhs_lines = line_spans_of(lhs).len();
    let rhs_lines = line_spans_of(rhs).len();

    let mut seen_lhs = vec![false; lhs_lines];
    let mut seen_rhs = vec![false; rhs_lines];

    for op in &diff.ops {
        match op {
            LineOp::Equal { lhs, rhs, .. } | LineOp::Replaced { lhs, rhs, .. } => {
                assert!(
                    !seen_lhs[*lhs],
                    "{}: lhs line {} covered twice",
                    path.display(),
                    lhs
                );
                assert!(
                    !seen_rhs[*rhs],
                    "{}: rhs line {} covered twice",
                    path.display(),
                    rhs
                );
                seen_lhs[*lhs] = true;
                seen_rhs[*rhs] = true;
            }
            LineOp::Deleted { lhs } => {
                assert!(
                    !seen_lhs[*lhs],
                    "{}: lhs line {} covered twice",
                    path.display(),
                    lhs
                );
                seen_lhs[*lhs] = true;
            }
            LineOp::Added { rhs } => {
                assert!(
                    !seen_rhs[*rhs],
                    "{}: rhs line {} covered twice",
                    path.display(),
                    rhs
                );
                seen_rhs[*rhs] = true;
            }
        }
    }

    assert!(
        seen_lhs.iter().all(|s| *s) && seen_rhs.iter().all(|s| *s),
        "{}: coverage violated\nops: {:#?}",
        path.display(),
        diff.ops
    );
}

#[test]
fn diff_source_lines_round_trip() {
    let lhs = "fn a() -> i32 {\n    1\n}\n";
    let rhs = "fn a() -> i32 {\n    2\n}\n";

    let diff = diff_source_lines(lhs, rhs, Path::new("x.rs"), None).expect("diff ok");

    assert!(diff
        .ops
        .iter()
        .any(|op| matches!(op, LineOp::Replaced { lhs: 1, rhs: 1, .. })));
}

#[test]
fn diff_source_lines_gate_and_fallback_cases() {
    // Unsupported extension.
    assert_eq!(
        diff_source_lines("a", "b", Path::new("x.bogus"), None),
        Err(FailedDiff::UnsupportedExtension("bogus".into()))
    );

    // No extension.
    assert_eq!(
        diff_source_lines("a", "b", Path::new("noext"), None),
        Err(FailedDiff::NoExtension)
    );

    // Heavily broken syntax → TooManyErrors, which callers use to fall back
    // to a plain line diff.
    let broken = "fn [[[ = = = {{{{\n\n(((( \n\n";
    match diff_source_lines(broken, broken, Path::new("x.rs"), None) {
        Err(FailedDiff::TooManyErrors { .. }) => {}
        other => panic!("expected TooManyErrors, got {other:?}"),
    }
}

#[test]
fn mildly_broken_syntax_still_diffs() {
    // One broken function among many stays under the coverage threshold and
    // must still produce a diff.
    let valid = (0..12)
        .map(|i| format!("fn generated_{i}() {{\n    {i}\n}}\n\n"))
        .collect::<String>();

    let lhs = format!("{valid}fn bad( = =\n");
    let rhs = format!(
        "{}fn bad( = =\n",
        (0..12)
            .map(|i| format!("fn generated_{i}() {{\n    {}\n}}\n\n", i + 1))
            .collect::<String>()
    );

    let diff =
        diff_source_lines(&lhs, &rhs, Path::new("x.rs"), None).expect("below coverage threshold");

    assert!(!diff.ops.is_empty());
}
