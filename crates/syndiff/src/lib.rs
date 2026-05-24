//! A library for computing syntax-aware diffs using tree-sitter.
//!
//! This crate provides a structural diff algorithm inspired by
//! [Difftastic](https://github.com/Wilfred/difftastic). Unlike line-based diffs,
//! it understands code structure and produces more meaningful results when
//! comparing source files.
//!
//! # Overview
//!
//! The primary types in this crate are:
//!
//! * [`diff_trees`]: Computes a diff between two syntax trees, returning byte
//!   ranges of changed regions.
//! * [`build_tree`]: Converts a tree-sitter parse tree into a [`SyntaxTree`].
//! * [`SyntaxTree`]: A syntax tree optimized for diffing, stored in preorder
//!   traversal.
//! * [`SyntaxDiffOptions`]: Configuration for the diff algorithm.
//!
//! # Example: basic diffing
//!
//! This example shows how to diff two Rust code snippets:
//!
//! ```
//! use syndiff::{build_tree, diff_trees};
//!
//! let old_source = "fn add(a: i32, b: i32) -> i32 { a + b }";
//! let new_source = "fn add(x: i32, y: i32) -> i32 { x + y }";
//!
//! // Parse with tree-sitter
//! let mut parser = tree_sitter::Parser::new();
//! parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
//!
//! let old_ts_tree = parser.parse(old_source, None).unwrap();
//! let new_ts_tree = parser.parse(new_source, None).unwrap();
//!
//! // Convert to syndiff trees
//! let old_tree = build_tree(old_ts_tree.walk(), old_source);
//! let new_tree = build_tree(new_ts_tree.walk(), new_source);
//!
//! // Compute the diff
//! let (old_ranges, new_ranges) = diff_trees(
//!     &old_tree,
//!     &new_tree,
//!     None,
//!     None,
//!     None,
//! ).expect("diff succeeded");
//!
//! // The ranges indicate which bytes changed
//! assert!(!old_ranges.is_empty());
//! assert!(!new_ranges.is_empty());
//!
//! // Extract the changed text
//! for range in &old_ranges {
//!     println!("removed: {:?}", &old_source[range.clone()]);
//! }
//! for range in &new_ranges {
//!     println!("added: {:?}", &new_source[range.clone()]);
//! }
//! ```
//!
//! # Example: limiting graph size
//!
//! For very different files, the diff graph can grow large. Use
//! [`SyntaxDiffOptions`] to set a limit:
//!
//! ```
//! use syndiff::{diff_trees, SyntaxDiffOptions};
//! # use syndiff::build_tree;
//! # let mut parser = tree_sitter::Parser::new();
//! # parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
//! # let old_tree = build_tree(parser.parse("fn a() {}", None).unwrap().walk(), "fn a() {}");
//! # let new_tree = build_tree(parser.parse("fn b() {}", None).unwrap().walk(), "fn b() {}");
//!
//! let options = SyntaxDiffOptions {
//!     graph_limit: 100_000, // Lower limit for faster failure
//! };
//!
//! match diff_trees(&old_tree, &new_tree, None, None, Some(options)) {
//!     Some((old_ranges, new_ranges)) => {
//!         println!("diff computed successfully");
//!     }
//!     None => {
//!         println!("diff exceeded graph limit, files too different");
//!     }
//! }
//! ```
//!
//! # Example: partial diffs with range bounds
//!
//! When diffing a specific region within larger files, provide byte range
//! bounds. The returned ranges will be clipped and made relative to the bounds:
//!
//! ```
//! use syndiff::{build_tree, diff_trees};
//! # let mut parser = tree_sitter::Parser::new();
//! # parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
//!
//! let old_source = "fn foo() { 1 }\nfn bar() { 2 }";
//! let new_source = "fn foo() { 1 }\nfn bar() { 3 }";
//!
//! let old_tree = build_tree(parser.parse(old_source, None).unwrap().walk(), old_source);
//! # parser.reset();
//! let new_tree = build_tree(parser.parse(new_source, None).unwrap().walk(), new_source);
//!
//! // Only diff the second function (bytes 15 onwards)
//! let (old_ranges, new_ranges) = diff_trees(
//!     &old_tree,
//!     &new_tree,
//!     Some(15..old_source.len()),
//!     Some(15..new_source.len()),
//!     None,
//! ).unwrap();
//!
//! // Ranges are relative to the bounds (starting from 0)
//! for range in &old_ranges {
//!     let absolute_range = (range.start + 15)..(range.end + 15);
//!     println!("changed in old: {:?}", &old_source[absolute_range]);
//! }
//! ```
//!
//! # How it works
//!
//! The algorithm is based on the approach described in the
//! [Difftastic manual](https://difftastic.wilfred.me.uk/), modeling tree
//! diffing as a shortest-path problem through an implicit graph.
//!
//! Each vertex represents a position in both syntax trees. Edges represent
//! diff operations (unchanged, added, removed) with associated costs tuned
//! to prefer matching identical subtrees. The algorithm finds the minimum-cost
//! path using Dijkstra's algorithm.
//!
//! ## Structural hashing
//!
//! Each node stores a 64-bit hash of its structure: the grammar kind, content
//! (for leaves), and children's hashes. This enables **O(1) subtree comparison** - if
//! two nodes have the same hash, their entire subtrees are identical and can
//! be skipped in one step.
//!
//! ## Complexity
//!
//! Let *n* and *m* be the number of nodes in the left and right trees.
//!
//! - **Time**: O(*n* × *m* × log(*n* × *m*)) worst case
//! - **Space**: O(*n* × *m*)
//!
//! In practice, structural hashing makes similar trees much faster - closer to
//! O(*n* + *m*) - since large unchanged subtrees are skipped entirely.
//!
//! The `graph_limit` parameter bounds memory usage by limiting vertices
//! explored, returning `None` if exceeded.

use std::ops::Range;

use arrayvec::ArrayVec;

use crate::syntax_graph::{SyntaxEdge, SyntaxRoute};
use crate::syntax_tree::{SyntaxHint, SyntaxNode};

mod syntax_delimiters;
mod syntax_graph;
mod syntax_tree;

pub use syntax_tree::{build_tree, SyntaxTree};

/// Configuration for the diff algorithm.
///
/// # Example
///
/// ```
/// use syndiff::SyntaxDiffOptions;
///
/// let options = SyntaxDiffOptions {
///     graph_limit: 500_000,
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct SyntaxDiffOptions {
    /// Maximum number of graph vertices to explore before giving up.
    ///
    /// The diff algorithm explores a graph where each vertex represents a
    /// position in both syntax trees. For very different files, this graph
    /// can grow exponentially. This limit prevents runaway memory usage.
    ///
    /// Default: `1_000_000`
    pub graph_limit: usize,
}

impl Default for SyntaxDiffOptions {
    fn default() -> Self {
        Self {
            graph_limit: 1_000_000,
        }
    }
}

/// Computes a syntax-aware diff between two syntax trees.
///
/// Returns `Some((lhs_ranges, rhs_ranges))` with byte ranges of changed regions,
/// or `None` if the diff graph exceeded the configured limit.
///
/// When `lhs_range` or `rhs_range` is provided, the returned ranges are clipped
/// to those bounds and made relative (starting from 0).
pub fn diff_trees(
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs_range: Option<Range<usize>>,
    rhs_range: Option<Range<usize>>,
    options: Option<SyntaxDiffOptions>,
) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    let options = options.unwrap_or_default();
    let route = syntax_graph::shortest_path(lhs_tree, rhs_tree, options.graph_limit)?;

    let (lhs_ranges, rhs_ranges) = collect_ranges(&route, lhs_tree, rhs_tree, lhs_range, rhs_range);

    Some((merge_ranges(lhs_ranges), merge_ranges(rhs_ranges)))
}

fn collect_ranges(
    route: &SyntaxRoute<'_>,
    lhs_tree: &SyntaxTree,
    rhs_tree: &SyntaxTree,
    lhs_bounds: Option<Range<usize>>,
    rhs_bounds: Option<Range<usize>>,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let mut lhs_ranges = Vec::default();
    let mut rhs_ranges = Vec::default();

    for path in &route.0 {
        let Some(edge) = path.edge else { continue };
        let Some(vertex) = path.from.as_ref() else {
            continue;
        };

        match edge {
            SyntaxEdge::Replaced { levenshtein_pct } => {
                if let (Some(lhs_node), Some(rhs_node)) = (
                    vertex.lhs.id().map(|id| lhs_tree.get(id)),
                    vertex.rhs.id().map(|id| rhs_tree.get(id)),
                ) {
                    if levenshtein_pct > 20 {
                        if let Some((lhs_replace_ranges, rhs_replace_ranges)) = get_replace_ranges(
                            lhs_node,
                            rhs_node,
                            lhs_bounds.as_ref(),
                            rhs_bounds.as_ref(),
                        ) {
                            lhs_ranges.extend(lhs_replace_ranges);
                            rhs_ranges.extend(rhs_replace_ranges);
                        }
                    } else {
                        lhs_ranges.extend(get_novel_ranges(lhs_node, lhs_bounds.as_ref()));
                        rhs_ranges.extend(get_novel_ranges(rhs_node, rhs_bounds.as_ref()));
                    }
                }
            }
            SyntaxEdge::NovelAtomLHS | SyntaxEdge::EnterNovelDelimiterLHS => {
                if let Some(lhs_node) = vertex.lhs.id().map(|id| lhs_tree.get(id)) {
                    lhs_ranges.extend(get_novel_ranges(lhs_node, lhs_bounds.as_ref()));
                }
            }
            SyntaxEdge::NovelAtomRHS | SyntaxEdge::EnterNovelDelimiterRHS => {
                if let Some(rhs_node) = vertex.rhs.id().map(|id| rhs_tree.get(id)) {
                    rhs_ranges.extend(get_novel_ranges(rhs_node, rhs_bounds.as_ref()));
                }
            }
            SyntaxEdge::EnterNovelDelimiterBoth => {
                if let Some(lhs_node) = vertex.lhs.id().map(|id| lhs_tree.get(id)) {
                    lhs_ranges.extend(get_novel_ranges(lhs_node, lhs_bounds.as_ref()));
                }
                if let Some(rhs_node) = vertex.rhs.id().map(|id| rhs_tree.get(id)) {
                    rhs_ranges.extend(get_novel_ranges(rhs_node, rhs_bounds.as_ref()));
                }
            }
            _ => {}
        }
    }

    (lhs_ranges, rhs_ranges)
}

fn get_novel_ranges(node: &SyntaxNode, bounds: Option<&Range<usize>>) -> ArrayVec<Range<usize>, 2> {
    let mut ranges = ArrayVec::new();

    if node.is_atom() {
        if let Some(r) = adjust_range_to_bounds(node.byte_range.clone(), bounds) {
            ranges.push(r);
        }
    } else {
        if let Some(r) = node
            .open_delimiter_range()
            .and_then(|r| adjust_range_to_bounds(r, bounds))
        {
            ranges.push(r);
        }

        if let Some(r) = node
            .close_delimiter_range()
            .and_then(|r| adjust_range_to_bounds(r, bounds))
        {
            ranges.push(r);
        }
    }

    ranges
}

fn get_replace_ranges(
    lhs_node: &SyntaxNode,
    rhs_node: &SyntaxNode,
    lhs_bounds: Option<&Range<usize>>,
    rhs_bounds: Option<&Range<usize>>,
) -> Option<(Vec<Range<usize>>, Vec<Range<usize>>)> {
    if let (Some(SyntaxHint::Comment(_lhs_comment)), Some(SyntaxHint::Comment(_rhs_comment))) =
        (lhs_node.hint.as_ref(), rhs_node.hint.as_ref())
    {
        // Convert relative ranges to absolute byte positions, then adjust to bounds
        let lhs_offset = lhs_node.byte_range.start;
        let rhs_offset = rhs_node.byte_range.start;

        let lhs_ranges = Vec::<Range<usize>>::default()
            .into_iter()
            .map(|r| (r.start + lhs_offset)..(r.end + lhs_offset))
            .filter_map(|r| adjust_range_to_bounds(r, lhs_bounds))
            .collect();

        let rhs_ranges = Vec::<Range<usize>>::default()
            .into_iter()
            .map(|r| (r.start + rhs_offset)..(r.end + rhs_offset))
            .filter_map(|r| adjust_range_to_bounds(r, rhs_bounds))
            .collect();

        Some((lhs_ranges, rhs_ranges))
    } else {
        None
    }
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return ranges;
    }

    ranges.sort_by_key(|r| r.start);
    let mut merged = vec![ranges[0].clone()];

    for range in ranges.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged is non-empty");
        if range.start <= last.end {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }

    merged
}

fn adjust_range_to_bounds(
    range: Range<usize>,
    bounds: Option<&Range<usize>>,
) -> Option<Range<usize>> {
    let Some(bounds) = bounds else {
        return Some(range);
    };

    if range.end <= bounds.start || range.start >= bounds.end {
        return None;
    }

    let start = range.start.max(bounds.start) - bounds.start;
    let end = range.end.min(bounds.end) - bounds.start;
    Some(start..end)
}
