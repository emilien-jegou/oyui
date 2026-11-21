//! Shared fixture/test helpers for the syndiff integration tests.

use std::ops::Range;
use std::path::{Path, PathBuf};

use oyui_syndiff::{build_tree, SyntaxTree};

pub fn parse_with_language(source: &str, language: tree_sitter::Language) -> SyntaxTree<'_> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("failed to set language");
    let tree = parser.parse(source, None).expect("failed to parse");
    build_tree(tree.walk(), source)
}

pub type TreeParser = fn(&str) -> SyntaxTree<'_>;

pub fn get_parser(lang: &str) -> TreeParser {
    match lang {
        "rust" => |source| parse_with_language(source, tree_sitter_rust::LANGUAGE.into()),
        "json" => |source| parse_with_language(source, tree_sitter_json::LANGUAGE.into()),
        _ => panic!("unsupported language: {lang}"),
    }
}

/// The language directory name of a fixture path (`.before` file).
pub fn fixture_pair(path: &Path) -> (String, String) {
    let before_content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));

    let after_path = path.with_extension("after");
    let after_content = std::fs::read_to_string(&after_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", after_path.display(), e));

    (before_content, after_content)
}

pub fn unmark_text(marked_text: &str) -> (String, Vec<Range<usize>>) {
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

/// A fixture bundle: path, language, both sides' texts and expected novel
/// ranges from the `«»` markers.
pub struct Fixture {
    pub path: PathBuf,
    pub lang: String,
    pub lhs: String,
    pub rhs: String,
    pub lhs_novel: Vec<Range<usize>>,
    pub rhs_novel: Vec<Range<usize>>,
}

/// Every fixture in the crate's `fixtures/` directory.
pub fn fixtures() -> Vec<Fixture> {
    let mut out = Vec::new();

    for entry in
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures")).expect("fixtures dir")
    {
        let lang_dir = entry.expect("fixture dir entry").path();
        if !lang_dir.is_dir() {
            continue;
        }

        let lang: String = lang_dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("lang name")
            .to_string();

        for file in std::fs::read_dir(&lang_dir).expect("lang dir") {
            let path = file.expect("fixture file").path();
            if path.extension().and_then(|e| e.to_str()) != Some("before") {
                continue;
            }

            let (before, after) = fixture_pair(&path);
            let (lhs, lhs_ranges) = unmark_text(&before);
            let (rhs, rhs_ranges) = unmark_text(&after);

            out.push(Fixture {
                path,
                lang: lang.clone(),
                lhs,
                rhs,
                lhs_novel: lhs_ranges,
                rhs_novel: rhs_ranges,
            });
        }
    }

    out
}
