use rstest::rstest;
use std::ops::Range;
use std::path::PathBuf;

use syndiff::{build_tree, diff_trees, SyntaxDiffOptions, SyntaxTree};

fn parse_with_language(source: &str, language: tree_sitter::Language) -> SyntaxTree<'_> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("failed to set language");
    let tree = parser.parse(source, None).expect("failed to parse");
    build_tree(tree.walk(), source)
}

fn mark_text(unmarked_text: &str, ranges: &[Range<usize>]) -> String {
    let mut marked_text = unmarked_text.to_string();
    for range in ranges.iter().rev() {
        marked_text.insert(range.end, '»');
        marked_text.insert(range.start, '«');
    }
    marked_text
}

fn unmark_text(marked_text: &str) -> (String, Vec<Range<usize>>) {
    let mut unmarked_text = String::new();
    let mut current_start = None;
    let mut ranges = Vec::default();
    let mut last_end = 0;

    for (marker_index, marker) in marked_text.match_indices(&['«', '»']) {
        unmarked_text.push_str(&marked_text[last_end..marker_index]);
        last_end = marker_index + marker.len();

        match marker {
            "«" => {
                if current_start.is_some() {
                    panic!("duplicate start marker at index {marker_index}")
                } else {
                    current_start = Some(unmarked_text.len());
                }
            }
            "»" => {
                if let Some(start) = current_start.take() {
                    ranges.push(start..unmarked_text.len());
                } else {
                    panic!("unexpected end marker at index {marker_index}")
                }
            }
            _ => unreachable!(),
        }
    }

    unmarked_text.push_str(&marked_text[last_end..]);
    (unmarked_text, ranges)
}

fn assert_diff(lhs_marked: &str, rhs_marked: &str, parser: fn(&str) -> SyntaxTree) {
    let (lhs_text, expected_lhs_ranges) = unmark_text(lhs_marked);
    let (rhs_text, expected_rhs_ranges) = unmark_text(rhs_marked);

    let lhs_tree = parser(&lhs_text);
    let rhs_tree = parser(&rhs_text);

    let (lhs_ranges, rhs_ranges) = diff_trees(
        &lhs_tree,
        &rhs_tree,
        None,
        None,
        Some(SyntaxDiffOptions::default()),
    )
    .expect("diff should not exceed graph limit");

    let actual_lhs_marked = mark_text(&lhs_text, &lhs_ranges);
    let actual_rhs_marked = mark_text(&rhs_text, &rhs_ranges);

    assert_eq!(
        lhs_ranges, expected_lhs_ranges,
        "LHS ranges mismatch.\nExpected: {lhs_marked}\nActual:   {actual_lhs_marked}"
    );
    assert_eq!(
        rhs_ranges, expected_rhs_ranges,
        "RHS ranges mismatch.\nExpected: {rhs_marked}\nActual:   {actual_rhs_marked}"
    );
}

fn get_parser(lang: &str) -> fn(&str) -> SyntaxTree {
    match lang {
        "rust" => |source| parse_with_language(source, tree_sitter_rust::LANGUAGE.into()),
        "json" => |source| parse_with_language(source, tree_sitter_json::LANGUAGE.into()),
        _ => panic!("unsupported language: {lang}"),
    }
}

#[rstest]
fn test_fixtures(#[files("fixtures/*/*.before")] path: PathBuf) {
    let lang = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .expect("could not determine language from path");

    let parser = get_parser(lang);

    let before_content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));

    let after_path = path.with_extension("after");
    let after_content = std::fs::read_to_string(&after_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", after_path.display(), e));

    assert_diff(&before_content, &after_content, parser);
}
