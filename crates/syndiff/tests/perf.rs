//! Performance sanity tests: the diff engines must terminate in bounded time
//! on large and degenerate inputs, and never corrupt their invariants at
//! scale.
//!
//! The wall-clock assertions are deliberately generous — they exist to catch
//! the pathological-hang class the `graph_limit` was introduced for (deeply
//! nested or wildly different files exploding the diff graph), not to pin
//! absolute performance. Timings are printed for local inspection.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use oyui_syndiff::{build_tree, diff_line_ops, LineDiff, LineOp, SyntaxDiffOptions, SyntaxTree};

/// Asserts `work()` finished within `bound` and prints the measured time.
fn within<T>(name: &str, bound: Duration, work: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let result = work();
    let elapsed = start.elapsed();

    eprintln!("{name}: {elapsed:?} (bound {bound:?})");
    assert!(
        elapsed < bound,
        "{name} took {elapsed:?}, bound {bound:?} — diff exploded",
    );

    result
}

/// Coverage invariant for big inputs: every line of both sides exactly once.
fn assert_coverage(diff: &LineDiff, lhs_lines: usize, rhs_lines: usize, name: &str) {
    let mut seen_lhs = vec![false; lhs_lines];
    let mut seen_rhs = vec![false; rhs_lines];

    for op in &diff.ops {
        match op {
            LineOp::Equal { lhs, rhs, .. } | LineOp::Replaced { lhs, rhs, .. } => {
                assert!(!seen_lhs[*lhs], "{name}: lhs line {lhs} twice");
                assert!(!seen_rhs[*rhs], "{name}: rhs line {rhs} twice");
                seen_lhs[*lhs] = true;
                seen_rhs[*rhs] = true;
            }
            LineOp::Deleted { lhs } => {
                assert!(!seen_lhs[*lhs], "{name}: lhs line {lhs} twice");
                seen_lhs[*lhs] = true;
            }
            LineOp::Added { rhs } => {
                assert!(!seen_rhs[*rhs], "{name}: rhs line {rhs} twice");
                seen_rhs[*rhs] = true;
            }
        }
    }

    assert!(
        seen_lhs.iter().all(|s| *s) && seen_rhs.iter().all(|s| *s),
        "{name}: coverage broken",
    );
}

fn count_lines(text: &str) -> usize {
    text.lines().count()
}

// ---------------------------------------------------------------------------
// Parsing helpers (generated sources leak into the test process deliberately:
// each is used inside its own test only).
// ---------------------------------------------------------------------------

fn parse_json(source: &'static str) -> SyntaxTree<'static> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_json::LANGUAGE.into())
        .expect("json language");
    let tree = parser.parse(source, None).expect("json parse");
    build_tree(tree.walk(), source)
}

fn parse_rust(source: &'static str) -> SyntaxTree<'static> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("rust language");
    let tree = parser.parse(source, None).expect("rust parse");
    build_tree(tree.walk(), source)
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

// ---------------------------------------------------------------------------
// Generators (deterministic, no RNG).
// ---------------------------------------------------------------------------

/// Deeply nested JSON containers; the deep change touches the value at the
/// innermost level. Nesting depth = `levels` (the root object + levels-1
/// children).
fn deep_json(levels: usize, deep_change: bool) -> String {
    let mut text = String::from("{\n");

    for level in 0..levels - 1 {
        let indent = " ".repeat(2 * (level + 1));
        let _ = writeln!(text, "{indent}\"level-{level}\": {{");
    }

    let innermost = " ".repeat(2 * levels);
    let _ = writeln!(
        text,
        "{innermost}\"leaf\": {}",
        if deep_change { 42 } else { 41 }
    );
    let _ = writeln!(
        text,
        "{innermost}\"sibling\": [{}]",
        (0..6).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
    );

    for level in (0..levels - 1).rev() {
        let indent = " ".repeat(2 * (level + 1));
        let _ = writeln!(text, "{indent}}},");
    }

    text.push_str("}\n");
    text
}

/// A realistic-ish JSON document with `entries` items, ten lines each.
fn big_json(entries: usize, rename_middle: bool, append_entry: bool) -> String {
    let mut text = String::from("{\n  \"items\": [\n");

    let last = if append_entry { entries } else { entries - 1 };
    for i in 0..=last {
        let name = if rename_middle && i == entries / 2 {
            "renamed-item"
        } else {
            "item"
        };
        let value = if rename_middle && i == entries / 2 {
            "3.5"
        } else {
            "3.14"
        };
        let comma = if i < last { "," } else { "" };

        if i >= entries {
            // The appended entry.
            let _ = write!(
                text,
                "    {{\n      \"id\": {i},\n      \"name\": \"appended-{i}\",\n      \"value\": 0.0,\n      \"tags\": [\"new\"],\n      \"meta\": {{\n        \"key\": \"appended\",\n        \"count\": {i}\n      }}\n    }}\n"
            );
            break;
        }

        let _ = write!(
            text,
            "    {{\n      \"id\": {i},\n      \"name\": \"{name}-{i}\",\n      \"value\": {value},\n      \"tags\": [\"alpha\", \"beta\"],\n      \"meta\": {{\n        \"key\": \"value\",\n        \"count\": {i}\n      }}\n    }}{comma}\n",
        );
    }

    text.push_str("  ]\n}\n");
    text
}

/// A generated Rust module with `count` functions; the parameters of the
/// middle function are renamed when `reparam` is set.
fn big_rust(count: usize, reparam: bool) -> String {
    let mut text = String::from("use std::ops::Add;\n\n");

    for i in 0..count {
        let (param, uses) = if reparam && i == count / 2 {
            ("value", "value")
        } else {
            ("x", "x")
        };

        let _ = write!(
            text,
            "/// Example function number {i}.\npub fn handle_{i}({param}: u32, y: u32) -> u32 {{\n    let a = {uses}.wrapping_add(y);\n    let b = {uses}.wrapping_mul({mult}).wrapping_add(a);\n    if b > 1000 {{\n        return b % 997;\n    }}\n    match b % 4 {{\n        0 => b / 2,\n        1 => b + 3,\n        2 => b - 1,\n        _ => b * 5,\n    }}\n}}\n\n",
            i = i,
            mult = i % 8 + 2,
        );
    }

    text
}

/// `text` reindented by one extra space on every non-empty line, keeping
/// every newline exactly (including trailing blank lines).
fn reindent(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);

    for chunk in text.split_inclusive('\n') {
        match chunk.strip_suffix('\n') {
            Some("") => out.push('\n'),
            Some(line) => {
                out.push(' ');
                out.push_str(line);
                out.push('\n');
            }
            None => {
                // Final chunk without a newline.
                if !chunk.is_empty() {
                    out.push(' ');
                }
                out.push_str(chunk);
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// The tests.
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_json_structural_hash_at_scale() {
    // Same deep file on both sides: no changes at all (and no
    // delimiter-stack corruption — the old packed u128 state panicked beyond
    // 16 levels, and huge depths used to explode the vertex state space).
    let lhs = deep_json(90, false);
    let rhs = deep_json(90, false);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let diff = within(
        "deep-json (identical, 90 levels)",
        Duration::from_secs(20),
        || {
            let (lt, rt) = (parse_json(lhs), parse_json(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None).expect("deep identical diff succeeds")
        },
    );

    assert_coverage(
        &diff,
        count_lines(lhs),
        count_lines(rhs),
        "deep-json-identical",
    );

    assert!(
        diff.ops.iter().all(|op| matches!(
            op,
            LineOp::Equal {
                identical: true,
                ..
            }
        )),
        "identical deep files must be fully equal"
    );
}

#[test]
fn probe_scaling() {
    for count in [5, 10, 20, 50, 200] {
        let lhs = big_rust(count, false);
        let rhs = big_rust(count, true);
        let (lt, rt) = (parse_rust(leak(lhs.clone())), parse_rust(leak(rhs.clone())));
        let nodes = lt.len().min(rt.len());
        let start = Instant::now();
        let res = diff_line_ops(&lt, &lhs, &rt, &rhs, Some(SyntaxDiffOptions::default()));
        eprintln!(
            "RUST count={count} nodes={nodes} -> {:?} in {:?}",
            res.map(|d| d.ops.len()),
            start.elapsed()
        );
    }
    for entries in [5, 20, 50] {
        let lhs = big_json(entries, false, false);
        let rhs = big_json(entries, true, true);
        let (lt, rt) = (parse_json(leak(lhs.clone())), parse_json(leak(rhs.clone())));
        let nodes = lt.len().min(rt.len());
        let start = Instant::now();
        let res = diff_line_ops(&lt, &lhs, &rt, &rhs, Some(SyntaxDiffOptions::default()));
        eprintln!(
            "JSON entries={entries} nodes={nodes} -> {:?} in {:?}",
            res.map(|d| d.ops.len()),
            start.elapsed()
        );
    }
}

#[test]
fn probe_reindent() {
    let lhs = big_rust(300, false);
    let rhs = reindent(&lhs);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let (lt, rt) = (parse_rust(lhs), parse_rust(rhs));
    let diff = diff_line_ops(&lt, lhs, &rt, rhs, None).expect("diff");
    eprintln!("coverage: {:?}", diff.covered_lines());

    for op in &diff.ops {
        if !matches!(op, LineOp::Equal { .. }) {
            eprintln!("NON EQUAL: {op:?}");
        }
    }
}

#[test]
fn deeply_nested_json_deep_change() {
    // 120 levels deep with the change at the innermost level: the walk has
    // to enter novel delimiter nesting without exploding.
    let lhs = deep_json(120, false);
    let rhs = deep_json(120, true);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let diff = within(
        "deep-json (deep change, 120 levels)",
        Duration::from_secs(30),
        || {
            let (lt, rt) = (parse_json(lhs), parse_json(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None).expect("deep diff succeeds")
        },
    );

    assert_coverage(
        &diff,
        count_lines(lhs),
        count_lines(rhs),
        "deep-json-change",
    );

    let changed = diff
        .ops
        .iter()
        .filter(|op| {
            !matches!(
                op,
                LineOp::Equal {
                    identical: true,
                    ..
                }
            )
        })
        .count();
    assert!(
        changed <= 4,
        "one innermost value change should not produce {changed} non-equal rows"
    );
}

#[test]
fn big_json_line_projection() {
    let entries = 600;
    let lhs = big_json(entries, false, false);
    let rhs = big_json(entries, true, true);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let diff = within(
        "big-json (~6k lines: rename + append)",
        Duration::from_secs(25),
        || {
            let (lt, rt) = (parse_json(lhs), parse_json(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None).expect("big json diff succeeds")
        },
    );

    assert_coverage(&diff, count_lines(lhs), count_lines(rhs), "big-json");

    // The rename must surface as a Replaced row with both sides' tokens
    // highlighted; the appended entry as plain additions.
    assert!(
        diff.ops.iter().any(|op| matches!(
            op,
            LineOp::Replaced { lhs_inline, rhs_inline, .. }
                if !lhs_inline.is_empty() && !rhs_inline.is_empty()
        )),
        "the rename should be a modified pair with highlights"
    );
    assert!(
        diff.ops.iter().any(|op| matches!(op, LineOp::Added { .. })),
        "the appended entry should produce additions"
    );
}

#[test]
fn big_rust_line_projection() {
    let count = 500;
    let lhs = big_rust(count, false);
    let rhs = big_rust(count, true);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let diff = within(
        "big-rust (~5.5k lines: renaming parameters in the middle)",
        Duration::from_secs(25),
        || {
            let (lt, rt) = (parse_rust(lhs), parse_rust(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None).expect("big rust diff succeeds")
        },
    );

    assert_coverage(&diff, count_lines(lhs), count_lines(rhs), "big-rust");

    // One function reparametrized: its line changes land in two rows
    // (declaration + first use); nothing else may churn.
    let changed = diff
        .ops
        .iter()
        .filter(|op| {
            !matches!(
                op,
                LineOp::Equal {
                    identical: true,
                    ..
                }
            )
        })
        .count();
    assert!(
        changed <= 8,
        "one reparametrized function should not produce {changed} non-equal rows"
    );
}

#[test]
fn big_rust_reindent_is_free() {
    let lhs = big_rust(300, false);
    let rhs = reindent(&lhs);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    let diff = within(
        "big-rust reindent (~3k lines)",
        Duration::from_secs(25),
        || {
            let (lt, rt) = (parse_rust(lhs), parse_rust(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None).expect("reindent diff succeeds")
        },
    );

    assert_coverage(
        &diff,
        count_lines(lhs),
        count_lines(rhs),
        "big-rust-reindent",
    );

    let has_deletions_or_additions = diff
        .ops
        .iter()
        .any(|op| matches!(op, LineOp::Deleted { .. } | LineOp::Added { .. }));
    assert!(
        !has_deletions_or_additions,
        "a pure reindentation must not produce deleted/added rows"
    );
}

#[test]
fn wildly_different_files_terminate_via_graph_limit() {
    // Incomparable files at scale: the graph-limit path has to cap the
    // search deterministically — bounded time, no panic, either a result or
    // None for the caller to fall back on.
    let lhs = big_json(400, false, false);
    let rhs = big_rust(400, false);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    within(
        "different files (graph-limit bound 30k)",
        Duration::from_secs(15),
        || {
            let (lt, rt) = (parse_json(lhs), parse_rust(rhs));
            let _ = diff_line_ops(
                &lt,
                lhs,
                &rt,
                rhs,
                Some(SyntaxDiffOptions {
                    graph_limit: 30_000,
                    ..Default::default()
                }),
            );
        },
    );
}

/// A function body of many small statements with edits interleaved dense:
/// every unchanged statement subtree sits below the unchanged-split
/// threshold, so the sectioned preprocessing aggregates the whole body into
/// ONE large conflict region (the rows.rs pathology that made a
/// syntax-aware diff walk ~1M states for ~10s before falling back).
///
/// The section budget must bound the walk and yield the line-diff fallback
/// in a blink instead.
#[test]
fn dense_small_statement_edits_bounded_by_section_budget() {
    // A body of small statements; the mutation edits identifiers inline
    // (same shape, novel tokens), which is exactly the class that used to
    // aggregate into one enormous conflict region.
    let body = |edit: bool| -> String {
        let mut text = String::from("fn work() {\n");
        for i in 0..500 {
            let name = if edit && i % 7 == 3 { "guard" } else { "step" };
            let value = if edit && i % 11 == 5 {
                i * 3 + 1
            } else {
                i * 3
            };
            text.push_str(&format!(
                "    let {name}_{i} = {value} + 1;\n    run_{i}({name}_{i});\n"
            ));
        }
        text.push_str("}\n");
        text
    };

    let lhs = body(false);
    let rhs = body(true);
    let (lhs, rhs) = (leak(lhs), leak(rhs));

    within(
        "dense small-statement edits (the rows.rs pathology)",
        Duration::from_secs(5),
        || {
            let (lt, rt) = (parse_rust(lhs), parse_rust(rhs));
            diff_line_ops(&lt, lhs, &rt, rhs, None)
        },
    );

    // Either a structural answer or the caller's fallback: the only
    // contract is bounded time.
}
