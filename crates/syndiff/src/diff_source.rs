use std::ops::Range;
use std::path::Path;

use crate::line_ops::LineDiff;
use crate::syntax_tree::SyntaxTree;
use crate::{build_tree, diff_line_ops, diff_trees, SyntaxDiffOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedDiff {
    NoExtension,
    UnsupportedExtension(String),
    ParserLanguageError(String),
    ParseFailed,
    GraphLimitExceeded,
    /// The parsed tree contains more `ERROR`/missing-token bytes than the
    /// configured coverage allows; a plain line diff will serve better.
    TooManyErrors {
        /// Fraction of source bytes under error nodes (both sides' maximum).
        coverage: u8,
    },
}

impl std::fmt::Display for FailedDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExtension => write!(f, "File has no extension"),
            Self::UnsupportedExtension(ext) => write!(f, "Unsupported extension: .{}", ext),
            Self::ParserLanguageError(err) => write!(f, "Failed to set parser language: {}", err),
            Self::ParseFailed => write!(f, "Tree-sitter failed to parse the source"),
            Self::GraphLimitExceeded => write!(f, "Diff graph limit exceeded"),
            Self::TooManyErrors { coverage } => write!(
                f,
                "Too many syntax errors ({}% of the file has unparseable content)",
                coverage
            ),
        }
    }
}

impl std::error::Error for FailedDiff {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxDiffResult {
    pub old_ranges: Vec<Range<usize>>,
    pub new_ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    #[cfg(feature = "rust")]
    Rust,
    #[cfg(feature = "python")]
    Python,
    #[cfg(feature = "go")]
    Go,
    #[cfg(feature = "javascript")]
    Javascript,
    #[cfg(feature = "typescript")]
    Typescript,
    #[cfg(feature = "typescript")]
    Tsx,
    #[cfg(feature = "c")]
    C,
    #[cfg(feature = "cpp")]
    Cpp,
    #[cfg(feature = "c-sharp")]
    CSharp,
    #[cfg(feature = "java")]
    Java,
    #[cfg(feature = "ruby")]
    Ruby,
    #[cfg(feature = "php")]
    Php,
    #[cfg(feature = "json")]
    Json,
    #[cfg(feature = "yaml")]
    Yaml,
    #[cfg(feature = "toml")]
    Toml,
    #[cfg(feature = "html")]
    Html,
    #[cfg(feature = "css")]
    Css,
    #[cfg(feature = "bash")]
    Bash,
}

impl SupportedLanguage {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            #[cfg(feature = "rust")]
            "rs" => Some(Self::Rust),
            #[cfg(feature = "python")]
            "py" => Some(Self::Python),
            #[cfg(feature = "go")]
            "go" => Some(Self::Go),
            #[cfg(feature = "javascript")]
            "js" | "mjs" | "cjs" => Some(Self::Javascript),
            #[cfg(feature = "typescript")]
            "ts" | "mts" | "cts" => Some(Self::Typescript),
            #[cfg(feature = "typescript")]
            "tsx" => Some(Self::Tsx),
            #[cfg(feature = "c")]
            "c" | "h" => Some(Self::C),
            #[cfg(feature = "cpp")]
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(Self::Cpp),
            #[cfg(feature = "c-sharp")]
            "cs" => Some(Self::CSharp),
            #[cfg(feature = "java")]
            "java" => Some(Self::Java),
            #[cfg(feature = "ruby")]
            "rb" => Some(Self::Ruby),
            #[cfg(feature = "php")]
            "php" => Some(Self::Php),
            #[cfg(feature = "json")]
            "json" => Some(Self::Json),
            #[cfg(feature = "yaml")]
            "yaml" | "yml" => Some(Self::Yaml),
            #[cfg(feature = "toml")]
            "toml" => Some(Self::Toml),
            #[cfg(feature = "html")]
            "html" | "htm" => Some(Self::Html),
            #[cfg(feature = "css")]
            "css" => Some(Self::Css),
            #[cfg(feature = "bash")]
            "sh" | "bash" => Some(Self::Bash),
            _ => None,
        }
    }

    pub fn get_ts_language(&self) -> tree_sitter::Language {
        match self {
            #[cfg(feature = "rust")]
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            #[cfg(feature = "python")]
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            #[cfg(feature = "go")]
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            #[cfg(feature = "javascript")]
            Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            #[cfg(feature = "typescript")]
            Self::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            #[cfg(feature = "typescript")]
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            #[cfg(feature = "c")]
            Self::C => tree_sitter_c::LANGUAGE.into(),
            #[cfg(feature = "cpp")]
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            #[cfg(feature = "c-sharp")]
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            #[cfg(feature = "java")]
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            #[cfg(feature = "ruby")]
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            #[cfg(feature = "php")]
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            #[cfg(feature = "json")]
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            #[cfg(feature = "yaml")]
            Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            #[cfg(feature = "toml")]
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            #[cfg(feature = "html")]
            Self::Html => tree_sitter_html::LANGUAGE.into(),
            #[cfg(feature = "css")]
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            #[cfg(feature = "bash")]
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
        }
    }
}

/// High-level function to parse and structurally diff two strings based on file extension.
pub fn diff_source(
    old_source: &str,
    new_source: &str,
    filepath: &Path,
    options: Option<SyntaxDiffOptions>,
) -> Result<SyntaxDiffResult, FailedDiff> {
    let (old_ts_tree, new_ts_tree, _) = parse_pair(old_source, new_source, filepath, options)?;

    let old_tree = build_tree(old_ts_tree.walk(), old_source);
    let new_tree = build_tree(new_ts_tree.walk(), new_source);

    let (old_ranges, new_ranges) = diff_trees(&old_tree, &new_tree, None, None, options)
        .ok_or(FailedDiff::GraphLimitExceeded)?;

    Ok(SyntaxDiffResult {
        old_ranges,
        new_ranges,
    })
}

/// High-level function to compute a structural, difftastic-style **line diff**
/// between two strings, based on file extension.
///
/// This is the entry point for the "full tree-sitter diffing" mode. It returns
/// one [`LineOp`] per display row; every line index of both sources occurs in
/// exactly one op, so a renderer can walk the ops directly and staging code can
/// reconstruct line-indexed patches.
///
/// The caller should render a plain text diff when this returns any
/// [`FailedDiff`] — `TooManyErrors` and `GraphLimitExceeded` in particular
/// indicate the structural walk would not help.
pub fn diff_source_lines(
    old_source: &str,
    new_source: &str,
    filepath: &Path,
    options: Option<SyntaxDiffOptions>,
) -> Result<LineDiff, FailedDiff> {
    let (old_ts_tree, new_ts_tree, _) = parse_pair(old_source, new_source, filepath, options)?;

    let old_tree = build_tree(old_ts_tree.walk(), old_source);
    let new_tree = build_tree(new_ts_tree.walk(), new_source);

    diff_line_ops(&old_tree, old_source, &new_tree, new_source, options)
        .ok_or(FailedDiff::GraphLimitExceeded)
}

/// Parses both sources and returns their diff-ready [`SyntaxTree`]s, applying
/// the same extension/error gates as [`diff_source`].
///
/// This is the entry point for callers that drive their own line alignment
/// (e.g. an imara line diff with token-level refinement): parse once, then
/// call [`crate::diff_inline`] per changed region.
pub fn parse_into_trees<'a>(
    old_source: &'a str,
    new_source: &'a str,
    filepath: &Path,
    options: Option<SyntaxDiffOptions>,
) -> Result<(SyntaxTree<'a>, SyntaxTree<'a>), FailedDiff> {
    let (old_ts_tree, new_ts_tree, _) = parse_pair(old_source, new_source, filepath, options)?;

    let old_tree = build_tree(old_ts_tree.walk(), old_source);
    let new_tree = build_tree(new_ts_tree.walk(), new_source);

    Ok((old_tree, new_tree))
}

/// Parses both sources for `filepath`, gating on:
///
/// * known extension → supported language,
/// * tree-sitter parse success,
/// * the fraction of bytes under `ERROR`/missing-token nodes being within
///   [`SyntaxDiffOptions::max_error_coverage`].
fn parse_pair<'t>(
    old_source: &'t str,
    new_source: &'t str,
    filepath: &Path,
    options: Option<SyntaxDiffOptions>,
) -> Result<(tree_sitter::Tree, tree_sitter::Tree, SupportedLanguage), FailedDiff> {
    let options = options.unwrap_or_default();

    let ext = filepath
        .extension()
        .and_then(|s| s.to_str())
        .ok_or(FailedDiff::NoExtension)?;

    let language = SupportedLanguage::from_extension(ext)
        .ok_or_else(|| FailedDiff::UnsupportedExtension(ext.to_string()))?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language.get_ts_language())
        .map_err(|e| FailedDiff::ParserLanguageError(e.to_string()))?;

    let old_ts_tree = parser
        .parse(old_source, None)
        .ok_or(FailedDiff::ParseFailed)?;
    let new_ts_tree = parser
        .parse(new_source, None)
        .ok_or(FailedDiff::ParseFailed)?;

    let coverage = error_coverage(&old_ts_tree, old_source);
    let new_coverage = error_coverage(&new_ts_tree, new_source);
    let coverage = coverage.max(new_coverage);

    if coverage > options.max_error_coverage {
        return Err(FailedDiff::TooManyErrors {
            coverage: (coverage * 100.0).round() as u8,
        });
    }

    Ok((old_ts_tree, new_ts_tree, language))
}

/// Fraction of bytes of `source` covered by tree-sitter `ERROR` or missing
/// token nodes. Empty sources are always considered clean.
fn error_coverage(tree: &tree_sitter::Tree, source: &str) -> f32 {
    if source.is_empty() {
        return 0.0;
    }

    let total = source.len();
    let mut covered = 0usize;

    // Preorder walk; error nodes count wholesale (their inner structure is
    // already recovery noise, so there is no point descending into them).
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            covered += node.byte_range().len();
            continue;
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }

    (covered as f32 / total as f32).clamp(0.0, 1.0)
}
