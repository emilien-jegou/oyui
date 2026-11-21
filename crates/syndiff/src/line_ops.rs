//! Tree-driven line alignment: converts a structural diff into line rows.
//!
//! The structural diff aligns subtrees; this module re-projects that
//! alignment onto the *lines* of both sources, producing the row model a
//! side-by-side diff view needs. The pipeline is a port of difftastic's line
//! conversion (`src/display/context.rs`), driven by per-token change marks
//! rather than line books:
//!
//! 1. Walk the diff route once and collect, per side: the byte ranges of
//!    *novel* tokens, and *anchor pairs* — line numbers paired through
//!    unchanged tokens. Unchanged atoms anchor their lines; unchanged
//!    delimiter-wrapped lists anchor their delimiter lines; structurally
//!    equal subtrees (which the graph skips as one edge) are re-walked in
//!    parallel so every inner token anchors its line.
//! 2. Pair anchored lines in order, merge in the lines that carry novel
//!    tokens, align blank lines preceding matched pairs, fill gaps and the
//!    file ends so every line of both sides shows in exactly one row
//!    (difftastic: `all_matched_lines_filled`).
//! 3. Project rows onto [`LineOp`]: anchored pairs become `Equal` (with
//!    line-relative token highlights when the row contains novel tokens),
//!    unpaired lines become `Deleted`/`Added`.
//!
//! # Invariant
//!
//! Every line index of both sides appears in exactly one op, in order, so the
//! row list is directly renderable, and staging code can reconstruct
//! line-indexed patches without re-deriving alignments.
//!
//! # Hooking this up
//!
//! The ops map onto a typical diff view row model:
//!
//! | [`LineOp`]               | view row                          |
//! |--------------------------|-----------------------------------|
//! | `Equal`                  | context row (both sides)          |
//! | `Equal` with inline      | context row with token highlights |
//! | `Deleted`                | deletion row (whole line novel)   |
//! | `Added`                  | addition row (whole line novel)   |
//!
//! When patch/staging construction cannot reconcile an `Equal` row that
//! carries highlights (or `identical == false`), it can be treated as an
//! aligned deletion/addition pair instead. That is the safety hatch which
//! keeps hunk selection (line toggles, hunk splits) working on modified
//! context rows without the patch scorer having to model whitespace rows.
//!
//! # Example
//!
//! ```
//! use oyui_syndiff::{build_tree, diff_line_ops};
//!
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
//!
//! let old = "fn a() -> i32 {\n    1\n}\n";
//! let new = "fn a() -> i32 {\n    2\n}\n";
//!
//! let old_tree = build_tree(parser.parse(old, None).unwrap().walk(), old);
//! # parser.reset();
//! let new_tree = build_tree(parser.parse(new, None).unwrap().walk(), new);
//!
//! let diff = diff_line_ops(&old_tree, old, &new_tree, new, None).unwrap();
//!
//! // Context for the unchanged lines and one modified pair with the novel
//! // token highlighted; every line appears exactly once.
//! assert_eq!(diff.ops.len(), 3);
//! assert_eq!(diff.covered_lines(), (3, 3));
//! ```

use std::cmp::Ordering;
use std::ops::Range;

use crate::syntax_graph::section_route;
use crate::syntax_tree::{SyntaxId, SyntaxNode, SyntaxTree};
use crate::SyntaxDiffOptions;

/// One aligned row of a structural line diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineOp {
    /// Row pairing an old line with a new line because a subtree covering
    /// them matched structurally.
    ///
    /// The common case is `identical: true`, no highlights. `identical:
    /// false` is kept for rows whose subtrees match but whose texts differ
    /// in trivia only (indentation, alignment): render as context, stage via
    /// the del/add hatch if the patch scorer cannot model them.
    Equal {
        lhs: usize,
        rhs: usize,
        /// The two line texts are exactly equal.
        identical: bool,
    },
    /// A modified pair: anchored rows that still carry novel tokens (the
    /// token-level change between otherwise-matching lines).
    ///
    /// The ranges are line-relative byte ranges of the novel tokens per
    /// side; an empty side range means that whole side's line is novel.
    Replaced {
        lhs: usize,
        rhs: usize,
        lhs_inline: Vec<Range<usize>>,
        rhs_inline: Vec<Range<usize>>,
    },
    /// An old line with no new counterpart: the whole line is novel.
    Deleted { lhs: usize },
    /// A new line with no old counterpart: the whole line is novel.
    Added { rhs: usize },
}

/// An aligned line-level diff derived from the structural tree diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineDiff {
    pub ops: Vec<LineOp>,
}

impl LineDiff {
    /// The number of old and new lines covered by the ops; handy for
    /// assertions and sanity checks.
    pub fn covered_lines(&self) -> (usize, usize) {
        let mut lhs = 0;
        let mut rhs = 0;
        for op in &self.ops {
            match op {
                LineOp::Equal { .. } | LineOp::Replaced { .. } => {
                    lhs += 1;
                    rhs += 1;
                }
                LineOp::Deleted { .. } => lhs += 1,
                LineOp::Added { .. } => rhs += 1,
            }
        }
        (lhs, rhs)
    }
}

/// Computes a structural, difftastic-style line diff between two sources
/// that were already parsed into [`SyntaxTree`]s.
///
/// Returns `None` when the structural diff exceeded the configured
/// [`SyntaxDiffOptions::graph_limit`]; callers should fall back to a plain
/// line diff in that case.
pub fn diff_line_ops(
    lhs_tree: &SyntaxTree,
    lhs_source: &str,
    rhs_tree: &SyntaxTree,
    rhs_source: &str,
    options: Option<SyntaxDiffOptions>,
) -> Option<LineDiff> {
    let options = options.unwrap_or_default();

    let lhs = Side::new(lhs_source);
    let rhs = Side::new(rhs_source);

    // 1) Novel token bytes and line anchors, from the sectioned
    //    preprocessing + per-section diff walks.
    let mut marks = collect_marks(lhs_tree, rhs_tree, &lhs, &rhs, &options)?;
    marks.lhs_novel = crate::merge_ranges(std::mem::take(&mut marks.lhs_novel));
    marks.rhs_novel = crate::merge_ranges(std::mem::take(&mut marks.rhs_novel));

    let lhs_novel_per_line = novel_per_line(&lhs, &marks.lhs_novel);
    let rhs_novel_per_line = novel_per_line(&rhs, &marks.rhs_novel);

    // Pair anchored lines, then fill: unpaired token lines, preceding
    // blanks, gaps, file ends.
    let mut matched: Vec<(usize, usize)> = Vec::new();

    // The deep walk emits a subtree's delimiters before its children, so the
    // raw anchors are walk-ordered, not line-ordered; sorting restores the
    // line order that the pairing pipeline requires.
    marks.anchors.sort_unstable();
    marks.anchors.dedup();

    for (l, r) in marks.anchors {
        let monotone = matched
            .last()
            .is_none_or(|(last_l, last_r)| l > *last_l && r > *last_r);
        if monotone {
            matched.push((l, r));
        }
    }

    let lhs_tokenized = tokenized_lines(&lhs, &lhs_novel_per_line, matched.iter().map(|(l, _)| *l));
    let rhs_tokenized = tokenized_lines(&rhs, &rhs_novel_per_line, matched.iter().map(|(_, r)| *r));

    let rows = all_matched_lines_filled(&matched, &rhs_tokenized, &lhs_tokenized, &lhs, &rhs);

    // 3) Project rows onto LineOp with per-line token highlights.
    let mut ops = Vec::with_capacity(rows.len());
    for (l, r) in rows {
        ops.push(match (l, r) {
            (Some(lhs_idx), Some(rhs_idx)) => {
                let identical = lhs.line_text(lhs_idx) == rhs.line_text(rhs_idx);
                let lhs_inline = lhs_novel_per_line[lhs_idx].clone();
                let rhs_inline = rhs_novel_per_line[rhs_idx].clone();

                if identical {
                    LineOp::Equal {
                        lhs: lhs_idx,
                        rhs: rhs_idx,
                        identical: true,
                    }
                } else if lhs_inline.is_empty() && rhs_inline.is_empty() {
                    // Tree-equal rows whose texts differ outside any token:
                    // trivia-only differences (indentation).
                    LineOp::Equal {
                        lhs: lhs_idx,
                        rhs: rhs_idx,
                        identical: false,
                    }
                } else {
                    // The row carries novel tokens (around unchanged ones):
                    // project as a modified pair.
                    LineOp::Replaced {
                        lhs: lhs_idx,
                        rhs: rhs_idx,
                        lhs_inline,
                        rhs_inline,
                    }
                }
            }
            (Some(lhs_idx), None) => LineOp::Deleted { lhs: lhs_idx },
            (None, Some(rhs_idx)) => LineOp::Added { rhs: rhs_idx },
            (None, None) => continue,
        });
    }

    Some(LineDiff { ops })
}

// ---------------------------------------------------------------------------
// Sides, lines and marks.
// ---------------------------------------------------------------------------

/// Byte spans of the lines of one source, with lookups.
struct Side<'a> {
    src: &'a str,
    spans: Vec<Range<usize>>,
}

impl<'a> Side<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            spans: line_spans(src),
        }
    }

    fn line_text(&self, idx: usize) -> &'a str {
        &self.src[self.spans[idx].clone()]
    }

    /// Line index containing byte `pos`.
    fn line_of(&self, pos: usize) -> usize {
        self.spans
            .partition_point(|span| span.end <= pos)
            .min(self.spans.len().saturating_sub(1))
    }
}

/// Byte ranges of every line of `content`, mirroring the line model used by
/// the rest of the diff pipeline: like `str::lines()`, a trailing `\r`
/// before a `\n` is stripped and a trailing newline produces no final empty
/// line.
fn line_spans(content: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    if content.is_empty() {
        return spans;
    }

    let mut start = 0usize;
    for raw in content.split('\n') {
        let raw_end = start + raw.len();

        // A `\r` before the terminator is not part of the line.
        let end = if raw.ends_with('\r') {
            raw_end - 1
        } else {
            raw_end
        };

        // `split` yields a final empty chunk for a trailing newline; that is
        // not a line.
        let is_tail = raw_end == content.len();
        if !(is_tail && end == start) {
            spans.push(start..end);
        }

        start = raw_end + 1;
    }

    spans
}

/// Novel token bytes and line anchors collected from the diff route.
#[derive(Default)]
struct Marks {
    lhs_novel: Vec<Range<usize>>,
    rhs_novel: Vec<Range<usize>>,
    /// Line pairs anchored by unchanged tokens, in walk order.
    anchors: Vec<(usize, usize)>,
}

// ---------------------------------------------------------------------------
// Sectioned preprocessing (port of difftastic's `mark_unchanged`): shrink
// obviously-equal subtrees, slice sibling lists by structural hash and only
// walk the small conflict regions that remain. This is what keeps the graph
// search linear-ish on files whose changes are local.
// ---------------------------------------------------------------------------

/// A section of siblings carved out of the tree pair.
#[derive(Debug)]
enum Section {
    /// Hash-equal subtree pairs: anchored, never walked.
    Equal { lhs: SyntaxId, rhs: SyntaxId },
    /// Lists whose delimiters match but whose children differ: the
    /// delimiters anchor; the children are sectioned recursively.
    EqualDelimiters { lhs: SyntaxId, rhs: SyntaxId },
    /// Sibling runs (contiguous preorder id windows) needing the diff walk.
    Conflict {
        lhs: Range<usize>,
        rhs: Range<usize>,
    },
    /// A run of siblings on only one side: everything in it is novel.
    OneSided { lhs: bool, siblings: Vec<SyntaxId> },
}

/// Unchanged sibling runs below this many nodes merge into the following
/// conflict region instead of splitting it (difftastic:
/// `TINY_TREE_THRESHOLD`).
const UNCHANGED_SPLIT_THRESHOLD: usize = 10;

fn split_pair(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs_id: SyntaxId,
    rhs_id: SyntaxId,
    out: &mut Vec<Section>,
) {
    let (lhs_node, rhs_node) = (lhs_tree.get(lhs_id), rhs_tree.get(rhs_id));

    if lhs_node.structural_hash == rhs_node.structural_hash {
        out.push(Section::Equal {
            lhs: lhs_id,
            rhs: rhs_id,
        });
        return;
    }

    // Same delimiters (including both absent, e.g. two `source_file`s):
    // recurse into the children, anchoring the delimiters when they exist.
    if lhs_node.is_list()
        && rhs_node.is_list()
        && (lhs_node.open_delimiter(), lhs_node.close_delimiter())
            == (rhs_node.open_delimiter(), rhs_node.close_delimiter())
    {
        if lhs_node.has_delimiters() {
            out.push(Section::EqualDelimiters {
                lhs: lhs_id,
                rhs: rhs_id,
            });
        }
        split_slices(
            lhs_tree,
            rhs_tree,
            &sibling_ids(lhs_tree, lhs_id),
            &sibling_ids(rhs_tree, rhs_id),
            out,
        );
        return;
    }

    // A compact pair of subtrees to diff wholesale.
    out.push(Section::Conflict {
        lhs: lhs_id.index()..lhs_id.index() + 1 + lhs_node.descendant_count,
        rhs: rhs_id.index()..rhs_id.index() + 1 + rhs_node.descendant_count,
    });
}

/// A conflict region accumulated from consecutive Left/Right runs and small
/// Both runs.
struct Region {
    lhs: Vec<SyntaxId>,
    rhs: Vec<SyntaxId>,
}

/// The direct children of `id`, in preorder order (each a complete subtree).
fn sibling_ids(tree: &SyntaxTree, id: SyntaxId) -> Vec<SyntaxId> {
    let mut out = Vec::new();
    let mut cur = tree.first_child(id);
    while let Some(child) = cur {
        out.push(child);
        cur = tree.next_sibling(child);
    }
    out
}

/// A run of the sibling-hash slice (indexes into the sibling lists).
enum Run {
    Both {
        lhs: Range<usize>,
        rhs: Range<usize>,
    },
    Left(Range<usize>),
    Right(Range<usize>),
}

/// Splits two sibling lists by structural-hash content, using imara's
/// histogram diff (difftastic: `lcs_diff::slice` + `split_unchanged`).
fn split_slices(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs: &[SyntaxId],
    rhs: &[SyntaxId],
    out: &mut Vec<Section>,
) {
    //    `shrink_at_ends`).
    let mut lhs_i = 0usize;
    let mut rhs_i = 0usize;

    while lhs_i < lhs.len()
        && rhs_i < rhs.len()
        && lhs_tree.get(lhs[lhs_i]).structural_hash == rhs_tree.get(rhs[rhs_i]).structural_hash
    {
        out.push(Section::Equal {
            lhs: lhs[lhs_i],
            rhs: rhs[rhs_i],
        });
        lhs_i += 1;
        rhs_i += 1;
    }

    let mut lhs_end = lhs.len();
    let mut rhs_end = rhs.len();

    while lhs_i < lhs_end
        && rhs_i < rhs_end
        && lhs_tree.get(lhs[lhs_end - 1]).structural_hash
            == rhs_tree.get(rhs[rhs_end - 1]).structural_hash
    {
        out.push(Section::Equal {
            lhs: lhs[lhs_end - 1],
            rhs: rhs[rhs_end - 1],
        });
        lhs_end -= 1;
        rhs_end -= 1;
    }

    // 2) One (or both) sides exhausted by the shrinking.
    if lhs_i >= lhs_end || rhs_i >= rhs_end {
        if lhs_i < lhs_end {
            out.push(Section::OneSided {
                lhs: true,
                siblings: lhs[lhs_i..lhs_end].to_vec(),
            });
        }
        if rhs_i < rhs_end {
            out.push(Section::OneSided {
                lhs: false,
                siblings: rhs[rhs_i..rhs_end].to_vec(),
            });
        }
        return;
    }

    // 3) Slice the remaining middles by structural hash.
    let lhs_hashes: Vec<u64> = (lhs_i..lhs_end)
        .map(|k| lhs_tree.get(lhs[k]).structural_hash)
        .collect();
    let rhs_hashes: Vec<u64> = (rhs_i..rhs_end)
        .map(|k| rhs_tree.get(rhs[k]).structural_hash)
        .collect();

    let mut region: Option<Region> = None;

    for run in slice_runs(&lhs_hashes, &rhs_hashes) {
        match run {
            Run::Both {
                lhs: run_lhs,
                rhs: run_rhs,
            } => {
                // Size the unchanged run by total subtree nodes
                // (difftastic: `is_big_enough`).
                let nodes: usize = run_lhs
                    .clone()
                    .map(|k| 1 + lhs_tree.get(lhs[lhs_i + k]).descendant_count)
                    .sum();

                if nodes >= UNCHANGED_SPLIT_THRESHOLD {
                    flush_region(&mut region, lhs_tree, rhs_tree, out);
                    for k in 0..run_lhs.len() {
                        out.push(Section::Equal {
                            lhs: lhs[lhs_i + run_lhs.start + k],
                            rhs: rhs[rhs_i + run_rhs.start + k],
                        });
                    }
                } else {
                    merge_into_region(
                        &mut region,
                        &lhs[lhs_i + run_lhs.start..lhs_i + run_lhs.end],
                        &rhs[rhs_i + run_rhs.start..rhs_i + run_rhs.end],
                    );
                }
            }
            Run::Left(range) => {
                merge_into_region(
                    &mut region,
                    &lhs[lhs_i + range.start..lhs_i + range.end],
                    &[],
                );
            }
            Run::Right(range) => {
                merge_into_region(
                    &mut region,
                    &[],
                    &rhs[rhs_i + range.start..rhs_i + range.end],
                );
            }
        }
    }

    flush_region(&mut region, lhs_tree, rhs_tree, out);
}

/// Accumulates consecutive siblings into the current conflict region; empty
/// slices leave the corresponding side untouched.
fn merge_into_region(region: &mut Option<Region>, lhs: &[SyntaxId], rhs: &[SyntaxId]) {
    let region = region.get_or_insert_with(|| Region {
        lhs: Vec::new(),
        rhs: Vec::new(),
    });

    if !lhs.is_empty() {
        region.lhs.extend_from_slice(lhs);
    }
    if !rhs.is_empty() {
        region.rhs.extend_from_slice(rhs);
    }
}

/// The contiguous preorder window covering sibling `run` (one or more
/// siblings, all descendants included): from the first sibling's index to
/// the last sibling's own id plus its descendants.
fn id_window(tree: &SyntaxTree, run: &[SyntaxId]) -> Range<usize> {
    let first = run.first().expect("non-empty sibling run").index();
    let last = run.last().expect("non-empty sibling run");
    first..last.index() + 1 + tree.get(*last).descendant_count
}

/// Flushes the accumulated region into a section; a single-pair region of
/// same-delimiter lists recurses into the children instead of being walked.
fn flush_region(
    region: &mut Option<Region>,
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    out: &mut Vec<Section>,
) {
    let Some(region) = region.take() else { return };

    match (region.lhs.is_empty(), region.rhs.is_empty()) {
        (false, false) if region.lhs.len() == 1 && region.rhs.len() == 1 => {
            split_pair(lhs_tree, rhs_tree, region.lhs[0], region.rhs[0], out);
        }
        (false, false) => {
            out.push(Section::Conflict {
                lhs: id_window(lhs_tree, &region.lhs),
                rhs: id_window(rhs_tree, &region.rhs),
            });
        }
        (false, true) => out.push(Section::OneSided {
            lhs: true,
            siblings: region.lhs,
        }),
        (true, false) => out.push(Section::OneSided {
            lhs: false,
            siblings: region.rhs,
        }),
        (true, true) => {}
    }
}

/// The histogram slice over sibling hashes.
fn slice_runs(lhs: &[u64], rhs: &[u64]) -> Vec<Run> {
    let mut input = imara_diff::InternedInput {
        before: Vec::with_capacity(lhs.len()),
        after: Vec::with_capacity(rhs.len()),
        interner: imara_diff::Interner::new(lhs.len() + rhs.len()),
    };
    input.update_before(lhs.iter());
    input.update_after(rhs.iter());

    let mut diff = imara_diff::Diff::compute(imara_diff::Algorithm::Histogram, &input);
    diff.postprocess_no_heuristic(&input);

    let mut runs = Vec::new();

    let mut lhs_i = 0usize;
    let mut rhs_i = 0usize;

    for hunk in diff.hunks() {
        let hb = hunk.before.start as usize;
        let ha = hunk.after.start as usize;

        if lhs_i < hb {
            runs.push(Run::Both {
                lhs: lhs_i..hb,
                rhs: rhs_i..ha,
            });
        }

        if hunk.before.end > hunk.before.start {
            runs.push(Run::Left(
                (hunk.before.start as usize)..(hunk.before.end as usize),
            ));
        }
        if hunk.after.end > hunk.after.start {
            runs.push(Run::Right(
                (hunk.after.start as usize)..(hunk.after.end as usize),
            ));
        }

        lhs_i = hunk.before.end as usize;
        rhs_i = hunk.after.end as usize;
    }

    if lhs_i < lhs.len() {
        runs.push(Run::Both {
            lhs: lhs_i..lhs.len(),
            rhs: rhs_i..rhs.len(),
        });
    }

    runs
}

/// Collects novel token bytes and line anchors for the whole tree pair via
/// the sectioned preprocessing.
fn collect_marks(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs: &Side,
    rhs: &Side,
    options: &SyntaxDiffOptions,
) -> Option<Marks> {
    let mut marks = Marks::default();
    let mut sections = Vec::new();

    let (lhs_root, rhs_root) = (lhs_tree.root()?, rhs_tree.root()?);
    split_pair(lhs_tree, rhs_tree, lhs_root, rhs_root, &mut sections);

    for section in &sections {
        match section {
            Section::Equal { lhs: l, rhs: r } => {
                deep_anchor_walk(lhs_tree, rhs_tree, *l, *r, lhs, rhs, &mut marks);
            }
            Section::EqualDelimiters { lhs: l, rhs: r } => {
                anchor_delimiter_pair(lhs_tree.get(*l), rhs_tree.get(*r), lhs, rhs, &mut marks);
            }
            Section::Conflict { lhs: l, rhs: r } => {
                let route = section_route(
                    lhs_tree,
                    rhs_tree,
                    SyntaxId::new(l.start),
                    l.end,
                    SyntaxId::new(r.start),
                    r.end,
                    options.graph_limit,
                )?;
                marks_from_route(lhs_tree, rhs_tree, &route, lhs, rhs, &mut marks);
            }
            Section::OneSided {
                lhs: is_lhs,
                siblings,
            } => {
                let tree = if *is_lhs { lhs_tree } else { rhs_tree };
                let out = if *is_lhs {
                    &mut marks.lhs_novel
                } else {
                    &mut marks.rhs_novel
                };
                for idx in id_window(tree, siblings) {
                    push_novel_ranges(tree.get(SyntaxId::new(idx)), out);
                }
            }
        }
    }

    Some(marks)
}

fn marks_from_route(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    route: &crate::syntax_graph::SyntaxRoute,
    lhs: &Side,
    rhs: &Side,
    marks: &mut Marks,
) {
    for path in &route.0 {
        let Some(edge) = path.edge else { continue };
        let Some(from) = path.from.as_ref() else {
            continue;
        };

        match edge {
            crate::syntax_graph::SyntaxEdge::Unchanged { .. } => {
                // Structural-equal atoms and whole equal subtrees: anchor
                // every token line on both sides.
                if let (Some(lhs_id), Some(rhs_id)) = (from.lhs.id(), from.rhs.id()) {
                    deep_anchor_walk(lhs_tree, rhs_tree, lhs_id, rhs_id, lhs, rhs, marks);
                }
            }
            crate::syntax_graph::SyntaxEdge::EnterUnchangedDelimiter { .. } => {
                // Delimiters of lists whose delimiters match but whose
                // children still differ; the route walks the children next.
                if let (Some(lhs_node), Some(rhs_node)) = (from.lhs.node(), from.rhs.node()) {
                    anchor_delimiter_pair(lhs_node, rhs_node, lhs, rhs, marks);
                }
            }
            crate::syntax_graph::SyntaxEdge::Replaced { .. } => {
                // Comments pair structurally but their text changed; mark
                // them novel so they surface as line changes.
                if let Some(lhs_node) = from.lhs.node() {
                    marks.lhs_novel.push(lhs_node.byte_range.clone());
                }
                if let Some(rhs_node) = from.rhs.node() {
                    marks.rhs_novel.push(rhs_node.byte_range.clone());
                }
            }
            crate::syntax_graph::SyntaxEdge::NovelAtomLHS
            | crate::syntax_graph::SyntaxEdge::EnterNovelDelimiterLHS => {
                if let Some(lhs_node) = from.lhs.node() {
                    push_novel_ranges(lhs_node, &mut marks.lhs_novel);
                }
            }
            crate::syntax_graph::SyntaxEdge::NovelAtomRHS
            | crate::syntax_graph::SyntaxEdge::EnterNovelDelimiterRHS => {
                if let Some(rhs_node) = from.rhs.node() {
                    push_novel_ranges(rhs_node, &mut marks.rhs_novel);
                }
            }
            crate::syntax_graph::SyntaxEdge::EnterNovelDelimiterBoth => {
                if let Some(lhs_node) = from.lhs.node() {
                    push_novel_ranges(lhs_node, &mut marks.lhs_novel);
                }
                if let Some(rhs_node) = from.rhs.node() {
                    push_novel_ranges(rhs_node, &mut marks.rhs_novel);
                }
            }
        }
    }
}

/// Novel ranges of a node: atoms span their bytes; delimiter-wrapped lists
/// span their open/close delimiters (children are marked separately).
fn push_novel_ranges(node: &SyntaxNode, out: &mut Vec<Range<usize>>) {
    if node.is_atom() && !node.has_delimiters() {
        out.push(node.byte_range.clone());
    } else if let (Some(open), Some(close)) =
        (node.open_delimiter_range(), node.close_delimiter_range())
    {
        out.push(open);
        out.push(close);
    }
}

/// Anchors the delimiter lines of a matched-delimiter list.
fn anchor_delimiter_pair(
    lhs_node: &SyntaxNode,
    rhs_node: &SyntaxNode,
    lhs: &Side,
    rhs: &Side,
    marks: &mut Marks,
) {
    if let (Some(lo), Some(ro)) = (
        lhs_node.open_delimiter_range(),
        rhs_node.open_delimiter_range(),
    ) {
        push_line_pair(lo.start, ro.start, lhs, rhs, marks);
    }
    if let (Some(lc), Some(rc)) = (
        lhs_node.close_delimiter_range(),
        rhs_node.close_delimiter_range(),
    ) {
        push_line_pair(lc.start, rc.start, lhs, rhs, marks);
    }
}

/// Walks two structurally-equal subtrees in parallel, anchoring every token
/// line. Equal hashes imply identical shape, so both walks stay in lockstep.
fn deep_anchor_walk(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs_id: SyntaxId,
    rhs_id: SyntaxId,
    lhs: &Side,
    rhs: &Side,
    marks: &mut Marks,
) {
    let lhs_node = lhs_tree.get(lhs_id);
    let rhs_node = rhs_tree.get(rhs_id);

    debug_assert_eq!(lhs_node.structural_hash, rhs_node.structural_hash);

    if lhs_node.is_atom() && !lhs_node.has_delimiters() {
        // A bare token: anchor its start line (and any other lines of a
        // multi-line token, whose counts match across equal subtrees).
        push_token_line_pairs(&lhs_node.byte_range, &rhs_node.byte_range, lhs, rhs, marks);
        return;
    }

    if lhs_node.has_delimiters() {
        anchor_delimiter_pair(lhs_node, rhs_node, lhs, rhs, marks);
    }

    let mut lhs_child = lhs_tree.first_child(lhs_id);
    let mut rhs_child = rhs_tree.first_child(rhs_id);

    while let (Some(lc), Some(rc)) = (lhs_child, rhs_child) {
        deep_anchor_walk(lhs_tree, rhs_tree, lc, rc, lhs, rhs, marks);
        lhs_child = lhs_tree.next_sibling(lc);
        rhs_child = rhs_tree.next_sibling(rc);
    }
}

/// Anchors every line a token covers.
fn push_token_line_pairs(
    lhs_range: &Range<usize>,
    rhs_range: &Range<usize>,
    lhs: &Side,
    rhs: &Side,
    marks: &mut Marks,
) {
    if lhs.spans.is_empty() || rhs.spans.is_empty() {
        return;
    }

    let lhs_first = lhs.line_of(lhs_range.start);
    let rhs_first = rhs.line_of(rhs_range.start);

    let lhs_last = lhs.line_of(lhs_range.end.saturating_sub(1).max(lhs_range.start));
    let rhs_last = rhs.line_of(rhs_range.end.saturating_sub(1).max(rhs_range.start));

    let count = (lhs_last - lhs_first + 1).min(rhs_last - rhs_first + 1);
    for k in 0..count {
        marks.anchors.push((lhs_first + k, rhs_first + k));
    }
}

/// Anchors the lines two byte positions sit on.
fn push_line_pair(lhs_pos: usize, rhs_pos: usize, lhs: &Side, rhs: &Side, marks: &mut Marks) {
    if lhs.spans.is_empty() || rhs.spans.is_empty() {
        return;
    }
    marks
        .anchors
        .push((lhs.line_of(lhs_pos), rhs.line_of(rhs_pos)));
}

// ---------------------------------------------------------------------------
// Line helpers.
// ---------------------------------------------------------------------------

/// Per-line novel byte ranges (line-relative), for lines that touch at
/// least one merged novel range. Empty for clean lines.
fn novel_per_line(side: &Side, novel: &[Range<usize>]) -> Vec<Vec<Range<usize>>> {
    let mut per_line: Vec<Vec<Range<usize>>> = vec![Vec::new(); side.spans.len()];

    let mut cursor = 0;
    for (i, line) in side.spans.iter().enumerate() {
        while cursor < novel.len() && novel[cursor].end <= line.start {
            cursor += 1;
        }
        let mut probe = cursor;
        while probe < novel.len() && novel[probe].start < line.end {
            let start = novel[probe].start.max(line.start);
            let end = novel[probe].end.min(line.end);
            if start < end {
                per_line[i].push((start - line.start)..(end - line.start));
            }
            probe += 1;
        }
    }

    per_line
}

/// Line indexes that carry at least one token: novel bytes or an anchor.
fn tokenized_lines(
    side: &Side,
    novel_per_line: &[Vec<Range<usize>>],
    anchor_lines: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let mut has_token = vec![false; side.spans.len()];

    for (i, ranges) in novel_per_line.iter().enumerate() {
        if !ranges.is_empty() {
            has_token[i] = true;
        }
    }
    for line in anchor_lines {
        has_token[line] = true;
    }

    (0..side.spans.len()).filter(|i| has_token[*i]).collect()
}

// ---------------------------------------------------------------------------
// Row pipeline: port of difftastic's display/context.rs.
// ---------------------------------------------------------------------------

/// The full row pipeline (difftastic: `all_matched_lines_filled`): merge in
/// unpaired tokenized lines, align preceding blanks, make rows contiguous
/// per side, compact gaps and add the file ends.
fn all_matched_lines_filled(
    matched: &[(usize, usize)],
    rhs_tokenized: &[usize],
    lhs_tokenized: &[usize],
    lhs: &Side,
    rhs: &Side,
) -> Vec<(Option<usize>, Option<usize>)> {
    // all_matched_lines: matched pairs, with the opposite side's unpaired
    // tokenized lines merged in; then the same in the other direction.
    let rows = merge_in_opposite_lines(&to_pairs(matched), rhs_tokenized);
    let rows = flip_tuples(&merge_in_opposite_lines(&flip_tuples(&rows), lhs_tokenized));

    let rows = match_preceding_blanks(&rows, lhs, rhs);
    let rows = ensure_contiguous(&rows);
    let rows = compact_gaps(&rows);
    add_ends(&rows, lhs.spans.len(), rhs.spans.len())
}

fn to_pairs(matched: &[(usize, usize)]) -> Vec<(Option<usize>, Option<usize>)> {
    matched.iter().map(|(l, r)| (Some(*l), Some(*r))).collect()
}

fn flip_tuples(rows: &[(Option<usize>, Option<usize>)]) -> Vec<(Option<usize>, Option<usize>)> {
    rows.iter().map(|(l, r)| (*r, *l)).collect()
}

/// Inserts tokenized lines of the opposite side that no matched pair
/// consumed (difftastic: `merge_in_opposite_lines`). Pairs must be in both
/// directions' sorted order; a pair's opposite line consumes the matching
/// tokenized line (the `Equal` branch) so it is never emitted as a single.
fn merge_in_opposite_lines(
    rows: &[(Option<usize>, Option<usize>)],
    opposite_tokenized: &[usize],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut res = Vec::new();

    let mut i = 0;
    for (line, opposite_line) in rows {
        if let Some(opposite_line) = opposite_line {
            while let Some(novel) = opposite_tokenized.get(i) {
                match novel.cmp(opposite_line) {
                    Ordering::Less => res.push((None, Some(*novel))),
                    Ordering::Equal => (),
                    Ordering::Greater => break,
                }
                i += 1;
            }
        }
        res.push((*line, *opposite_line));
    }

    while let Some(novel) = opposite_tokenized.get(i) {
        res.push((None, Some(*novel)));
        i += 1;
    }

    res
}

/// Aligns blank lines that precede matched pairs, on both sides
/// (difftastic: `match_preceding_blanks`; only blanks are candidates, plus
/// the first pair may pull in leading blanks).
fn match_preceding_blanks(
    rows: &[(Option<usize>, Option<usize>)],
    lhs: &Side,
    rhs: &Side,
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut prev_lhs: Option<usize> = None;
    let mut prev_rhs: Option<usize> = None;

    let mut res: Vec<(Option<usize>, Option<usize>)> = Vec::new();

    for (lhs_line, rhs_line) in rows {
        if let (Some(l), Some(r)) = (lhs_line, rhs_line) {
            let blanks = match (prev_lhs, prev_rhs) {
                (Some(pl), Some(pr)) => match_blanks_between((*l, *r), (pl, pr), lhs, rhs),
                _ => match_blanks_before((*l, *r), lhs, rhs),
            };
            res.extend(blanks);
        }

        res.push((*lhs_line, *rhs_line));

        if lhs_line.is_some() {
            prev_lhs = *lhs_line;
        }
        if rhs_line.is_some() {
            prev_rhs = *rhs_line;
        }
    }

    res
}

/// Blank lines between the previous matched pair and `current`.
fn match_blanks_between(
    current: (usize, usize),
    prev: (usize, usize),
    lhs: &Side,
    rhs: &Side,
) -> Vec<(Option<usize>, Option<usize>)> {
    if current.0 == 0 || current.1 == 0 {
        return vec![];
    }

    let mut cur = (current.0 - 1, current.1 - 1);
    let mut res = vec![];

    while cur.0 > prev.0 && cur.1 > prev.1 {
        if lhs.line_text(cur.0).trim().is_empty() && rhs.line_text(cur.1).trim().is_empty() {
            res.push((Some(cur.0), Some(cur.1)));
            if cur.0 == 0 || cur.1 == 0 {
                break;
            }
            cur = (cur.0 - 1, cur.1 - 1);
        } else {
            break;
        }
    }

    res.reverse();
    res
}

/// Blank lines before the first matched pair.
fn match_blanks_before(
    current: (usize, usize),
    lhs: &Side,
    rhs: &Side,
) -> Vec<(Option<usize>, Option<usize>)> {
    if current.0 == 0 || current.1 == 0 {
        return vec![];
    }

    let mut cur = (current.0 - 1, current.1 - 1);
    let mut res = vec![];

    loop {
        if lhs.line_text(cur.0).trim().is_empty() && rhs.line_text(cur.1).trim().is_empty() {
            res.push((Some(cur.0), Some(cur.1)));
            if cur.0 == 0 || cur.1 == 0 {
                break;
            }
            cur = (cur.0 - 1, cur.1 - 1);
        } else {
            break;
        }
    }

    res.reverse();
    res
}

/// Adds the single-side rows implied by gaps between matched lines
/// (difftastic: `ensure_contiguous`).
fn ensure_contiguous(
    rows: &[(Option<usize>, Option<usize>)],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut res: Vec<(Option<usize>, Option<usize>)> = Vec::new();

    let mut lhs_min: Option<usize> = None;
    let mut rhs_min: Option<usize> = None;

    for (lhs_line, rhs_line) in rows {
        if let Some(lhs_line) = lhs_line {
            if let Some(lhs_prev) = lhs_min {
                if lhs_prev + 1 != *lhs_line {
                    res.extend(((lhs_prev + 1)..*lhs_line).map(|l| (Some(l), None)));
                }
            }
            lhs_min = Some(*lhs_line);
        }
        if let Some(rhs_line) = rhs_line {
            if let Some(rhs_prev) = rhs_min {
                if rhs_prev + 1 != *rhs_line {
                    res.extend(((rhs_prev + 1)..*rhs_line).map(|r| (None, Some(r))));
                }
            }
            rhs_min = Some(*rhs_line);
        }
        res.push((*lhs_line, *rhs_line));
    }

    res
}

/// Aligns single-side rows that face each other across a gap
/// (difftastic: `compact_gaps`).
fn compact_gaps(rows: &[(Option<usize>, Option<usize>)]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut res: Vec<(Option<usize>, Option<usize>)> = Vec::new();

    // Single-side rows waiting for a partner, all on the same side.
    let mut unpaired: Vec<(Option<usize>, Option<usize>)> = Vec::new();

    for (lhs_line, rhs_line) in rows {
        match (lhs_line, rhs_line) {
            (Some(lhs_line), None) => match unpaired.first() {
                Some((None, Some(_))) => {
                    res.push((Some(*lhs_line), unpaired.remove(0).1));
                }
                _ => unpaired.push((Some(*lhs_line), None)),
            },
            (None, Some(rhs_line)) => match unpaired.first() {
                Some((Some(_), None)) => {
                    res.push((unpaired.remove(0).0, Some(*rhs_line)));
                }
                _ => unpaired.push((None, Some(*rhs_line))),
            },
            _ => {
                res.append(&mut unpaired);
                res.push((*lhs_line, *rhs_line));
            }
        }
    }

    res.append(&mut unpaired);
    res
}

/// Extends rows to include the leading and trailing lines of the files
/// (difftastic: `add_ends`).
fn add_ends(
    rows: &[(Option<usize>, Option<usize>)],
    lhs_len: usize,
    rhs_len: usize,
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut res = vec![];

    let lhs_min = rows.iter().find_map(|(l, _)| *l);
    let rhs_min = rows.iter().find_map(|(_, r)| *r);

    if let (Some(lhs_min), Some(rhs_min)) = (lhs_min, rhs_min) {
        let mut lhs_line = 0;
        let mut rhs_line = 0;
        while lhs_line < lhs_min && rhs_line < rhs_min {
            res.push((Some(lhs_line), Some(rhs_line)));
            lhs_line += 1;
            rhs_line += 1;
        }
        while lhs_line < lhs_min {
            res.push((Some(lhs_line), None));
            lhs_line += 1;
        }
        while rhs_line < rhs_min {
            res.push((None, Some(rhs_line)));
            rhs_line += 1;
        }
    }

    res.extend_from_slice(rows);

    let lhs_max = rows.iter().rev().find_map(|(l, _)| *l);
    let rhs_max = rows.iter().rev().find_map(|(_, r)| *r);

    if let (Some(lhs_max), Some(rhs_max)) = (lhs_max, rhs_max) {
        let mut lhs_line = lhs_max + 1;
        let mut rhs_line = rhs_max + 1;
        while lhs_line < lhs_len && rhs_line < rhs_len {
            res.push((Some(lhs_line), Some(rhs_line)));
            lhs_line += 1;
            rhs_line += 1;
        }
        while lhs_line < lhs_len {
            res.push((Some(lhs_line), None));
            lhs_line += 1;
        }
        while rhs_line < rhs_len {
            res.push((None, Some(rhs_line)));
            rhs_line += 1;
        }
    }

    res
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

    fn diff(lhs: &str, rhs: &str) -> LineDiff {
        let (lt, rt) = (parse(lhs), parse(rhs));
        diff_line_ops(&lt, lhs, &rt, rhs, None).expect("graph limit not hit")
    }

    /// Renders each op as a compact assertion-friendly string; rows carrying
    /// inline highlights show their line with `«»` markers around the
    /// highlighted spans.
    fn render(diff: &LineDiff, lhs: &str, rhs: &str) -> Vec<String> {
        let lhs_spans = line_spans(lhs);
        let rhs_spans = line_spans(rhs);

        let marked = |text: &str, span: &Range<usize>, inline: &[Range<usize>]| {
            if inline.is_empty() {
                return text[span.clone()].to_string();
            }
            let mut marked = text[span.clone()].to_string();
            for range in inline.iter().rev() {
                marked.insert(range.end, '»');
                marked.insert(range.start, '«');
            }
            marked
        };

        diff.ops
            .iter()
            .map(|op| match op {
                LineOp::Equal {
                    lhs: l,
                    rhs: r,
                    identical,
                } => {
                    format!("= {l}:{r}{}", if *identical { "" } else { " (ws)" })
                }
                LineOp::Replaced {
                    lhs: l,
                    rhs: r,
                    lhs_inline,
                    rhs_inline,
                } => format!(
                    "~ {l}:{r} [{}] [{}]",
                    marked(lhs, &lhs_spans[*l], lhs_inline),
                    marked(rhs, &rhs_spans[*r], rhs_inline),
                ),
                LineOp::Deleted { lhs: l } => format!("- {l}:{}", &lhs[lhs_spans[*l].clone()]),
                LineOp::Added { rhs: r } => format!("+ :{}", &rhs[rhs_spans[*r].clone()]),
            })
            .collect()
    }

    /// Asserts the full coverage + monotonicity invariant for a diff.
    fn assert_invariant(d: &LineDiff, lhs: &str, rhs: &str) {
        let lhs_lines = line_spans(lhs).len();
        let rhs_lines = line_spans(rhs).len();

        let mut seen_lhs = vec![false; lhs_lines];
        let mut seen_rhs = vec![false; rhs_lines];

        let mut lhs_cursor = 0;
        let mut rhs_cursor = 0;

        for op in &d.ops {
            match op {
                LineOp::Equal { lhs, rhs, .. } | LineOp::Replaced { lhs, rhs, .. } => {
                    assert!(*lhs >= lhs_cursor, "lhs order: {d:?}");
                    assert!(*rhs >= rhs_cursor, "rhs order: {d:?}");
                    seen_lhs[*lhs] = true;
                    seen_rhs[*rhs] = true;
                    lhs_cursor = *lhs + 1;
                    rhs_cursor = *rhs + 1;
                }
                LineOp::Deleted { lhs } => {
                    assert!(*lhs >= lhs_cursor, "lhs order: {d:?}");
                    seen_lhs[*lhs] = true;
                    lhs_cursor = *lhs + 1;
                }
                LineOp::Added { rhs } => {
                    assert!(*rhs >= rhs_cursor, "rhs order: {d:?}");
                    seen_rhs[*rhs] = true;
                    rhs_cursor = *rhs + 1;
                }
            }
        }

        assert!(
            seen_lhs.iter().all(|s| *s) && seen_rhs.iter().all(|s| *s),
            "coverage violated for:\n{lhs}\n---\n{rhs}\nops:\n{}",
            d.ops
                .iter()
                .map(|op| format!("{op:?}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    #[test]
    fn single_line_token_edit_is_del_add_pair_with_token_highlights() {
        let lhs = "fn a() -> i32 {\n    1\n}\n";
        let rhs = "fn a() -> i32 {\n    2\n}\n";

        let d = diff(lhs, rhs);

        assert_eq!(
            render(&d, lhs, rhs),
            vec!["= 0:0", "~ 1:1 [    «1»] [    «2»]", "= 2:2",]
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn reindentation_pairs_as_context() {
        let lhs = "fn a() {\n    work();\n}\n";
        let rhs = "    fn a() {\n        work();\n    }\n";

        let d = diff(lhs, rhs);

        assert_eq!(
            render(&d, lhs, rhs),
            vec!["= 0:0 (ws)", "= 1:1 (ws)", "= 2:2 (ws)",]
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn added_function_is_added_rows() {
        let lhs = "fn one() {}\n";
        let rhs = "fn one() {}\nfn one() {}\n";

        let d = diff(lhs, rhs);

        assert_eq!(render(&d, lhs, rhs), vec!["= 0:0", "+ :fn one() {}",]);
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn blank_line_between_functions_deletes() {
        let lhs = "fn a() {}\n\nfn b() {}\n";
        let rhs = "fn a() {}\nfn b() {}\n";

        let d = diff(lhs, rhs);

        assert_eq!(render(&d, lhs, rhs), vec!["= 0:0", "- 1:", "= 2:1",]);
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn comment_change_is_del_add_pair() {
        let lhs = "// old\nfn a() {}\n";
        let rhs = "// new\nfn a() {}\n";

        let d = diff(lhs, rhs);

        assert_eq!(
            render(&d, lhs, rhs),
            vec!["~ 0:0 [«// old»] [«// new»]", "= 1:1",]
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn mixed_line_keeps_anchor_with_token_highlights() {
        let lhs = "let value = compute(foo, 1);\n";
        let rhs = "let value = compute(bar, 1);\n";

        let d = diff(lhs, rhs);

        // The line anchors on the unchanged tokens and highlights only the
        // novel ones; the row is a modified pair.
        assert_eq!(
            render(&d, lhs, rhs),
            vec!["~ 0:0 [let value = compute(«foo», 1);] [let value = compute(«bar», 1);]"]
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn wrapping_change_still_aligns_body() {
        let lhs = "fn a(x: i32) -> i32 {\n    x\n}\n";
        let rhs = "fn a(\n    x: i32,\n) -> i32 {\n    x\n}\n";

        let d = diff(lhs, rhs);

        // Signature rows pair as tree-equal-but-different; the wrapped
        // argument tail is added; the body and closing brace stay equal.
        assert_eq!(
            d.ops,
            vec![
                LineOp::Equal {
                    lhs: 0,
                    rhs: 0,
                    identical: false
                },
                LineOp::Added { rhs: 1 },
                LineOp::Added { rhs: 2 },
                LineOp::Equal {
                    lhs: 1,
                    rhs: 3,
                    identical: true
                },
                LineOp::Equal {
                    lhs: 2,
                    rhs: 4,
                    identical: true
                },
            ]
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn empty_input_yields_additions() {
        let d = diff("", "fn put() {}\n");
        assert_eq!(d.ops.len(), 1);
        assert!(matches!(d.ops[0], LineOp::Added { rhs: 0 }));

        let d = diff("fn del() {}\n", "");
        assert_eq!(d.ops.len(), 1);
        assert!(matches!(d.ops[0], LineOp::Deleted { lhs: 0 }));

        let d = diff("", "");
        assert!(d.ops.is_empty());
    }

    #[test]
    fn fully_different_files_have_no_equal_rows() {
        let lhs = "fn a() {\n}\n";
        let rhs = "x: [{\"k\": 1}]\n";

        let d = diff(lhs, rhs);

        // The shared `{`/`x`/`:` tokens pair the first lines as a modified
        // pair with token highlights; only clean-identical rows must be
        // absent.
        assert!(
            d.ops.iter().all(|op| !matches!(
                op,
                LineOp::Equal {
                    identical: true,
                    ..
                }
            )),
            "nothing can be identical: {d:?}"
        );
        assert_invariant(&d, lhs, rhs);
    }

    #[test]
    fn coverage_invariant_holds_over_matrix() {
        for src in [
            "fn a() -> i32 {\n    1\n}\n",
            "fn a() -> i32 {\n    2\n}\n",
            "fn a() {}\n\nfn b() {}\n",
            "fn a() {}\n",
            "    fn a() {\n        work();\n    }\n",
            "x: [{\"k\": 1}]\n",
        ] {
            for dst in [
                "fn a() -> i32 {\n    1\n}\n",
                "fn a() -> i32 {\n    2\n}\n",
                "fn one() {}\nfn one() {}\n",
                "x: [{\"k\": 2}]\n",
                "fn a(\n    x: i32,\n) -> i32 {\n    x\n}\n",
                "",
                "/// doc\npub struct S { pub a: u8, pub b: Vec<String> }\n\nimpl S {\n    async fn go(&self) -> eyre::Result<()> {\n        self.a = 3; Ok(())\n    }\n}\n",
            ] {
                let d = diff(src, dst);
                assert_invariant(&d, src, dst);
            }
        }
    }
}
