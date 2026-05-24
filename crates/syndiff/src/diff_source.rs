use std::path::Path;
use std::ops::Range;

use crate::{build_tree, diff_trees, SyntaxDiffOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailedDiff {
    NoExtension,
    UnsupportedExtension(String),
    ParserLanguageError(String),
    ParseFailed,
    GraphLimitExceeded,
}

impl std::fmt::Display for FailedDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExtension => write!(f, "File has no extension"),
            Self::UnsupportedExtension(ext) => write!(f, "Unsupported extension: .{}", ext),
            Self::ParserLanguageError(err) => write!(f, "Failed to set parser language: {}", err),
            Self::ParseFailed => write!(f, "Tree-sitter failed to parse the source"),
            Self::GraphLimitExceeded => write!(f, "Diff graph limit exceeded"),
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
    #[cfg(feature = "rust")] Rust,
    #[cfg(feature = "python")] Python,
    #[cfg(feature = "go")] Go,
    #[cfg(feature = "javascript")] Javascript,
    #[cfg(feature = "typescript")] Typescript,
    #[cfg(feature = "typescript")] Tsx,
    #[cfg(feature = "c")] C,
    #[cfg(feature = "cpp")] Cpp,
    #[cfg(feature = "c-sharp")] CSharp,
    #[cfg(feature = "java")] Java,
    #[cfg(feature = "ruby")] Ruby,
    #[cfg(feature = "php")] Php,
    #[cfg(feature = "json")] Json,
    #[cfg(feature = "yaml")] Yaml,
    #[cfg(feature = "toml")] Toml,
    #[cfg(feature = "html")] Html,
    #[cfg(feature = "css")] Css,
    #[cfg(feature = "bash")] Bash,
}

impl SupportedLanguage {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            #[cfg(feature = "rust")] "rs" => Some(Self::Rust),
            #[cfg(feature = "python")] "py" => Some(Self::Python),
            #[cfg(feature = "go")] "go" => Some(Self::Go),
            #[cfg(feature = "javascript")] "js" | "mjs" | "cjs" => Some(Self::Javascript),
            #[cfg(feature = "typescript")] "ts" | "mts" | "cts" => Some(Self::Typescript),
            #[cfg(feature = "typescript")] "tsx" => Some(Self::Tsx),
            #[cfg(feature = "c")] "c" | "h" => Some(Self::C),
            #[cfg(feature = "cpp")] "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(Self::Cpp),
            #[cfg(feature = "c-sharp")] "cs" => Some(Self::CSharp),
            #[cfg(feature = "java")] "java" => Some(Self::Java),
            #[cfg(feature = "ruby")] "rb" => Some(Self::Ruby),
            #[cfg(feature = "php")] "php" => Some(Self::Php),
            #[cfg(feature = "json")] "json" => Some(Self::Json),
            #[cfg(feature = "yaml")] "yaml" | "yml" => Some(Self::Yaml),
            #[cfg(feature = "toml")] "toml" => Some(Self::Toml),
            #[cfg(feature = "html")] "html" | "htm" => Some(Self::Html),
            #[cfg(feature = "css")] "css" => Some(Self::Css),
            #[cfg(feature = "bash")] "sh" | "bash" => Some(Self::Bash),
            _ => None,
        }
    }

    pub fn get_ts_language(&self) -> tree_sitter::Language {
        match self {
            #[cfg(feature = "rust")] Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            #[cfg(feature = "python")] Self::Python => tree_sitter_python::LANGUAGE.into(),
            #[cfg(feature = "go")] Self::Go => tree_sitter_go::LANGUAGE.into(),
            #[cfg(feature = "javascript")] Self::Javascript => tree_sitter_javascript::LANGUAGE.into(),
            #[cfg(feature = "typescript")] Self::Typescript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            #[cfg(feature = "typescript")] Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            #[cfg(feature = "c")] Self::C => tree_sitter_c::LANGUAGE.into(),
            #[cfg(feature = "cpp")] Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            #[cfg(feature = "c-sharp")] Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            #[cfg(feature = "java")] Self::Java => tree_sitter_java::LANGUAGE.into(),
            #[cfg(feature = "ruby")] Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            #[cfg(feature = "php")] Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            #[cfg(feature = "json")] Self::Json => tree_sitter_json::LANGUAGE.into(),
            #[cfg(feature = "yaml")] Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            #[cfg(feature = "toml")] Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            #[cfg(feature = "html")] Self::Html => tree_sitter_html::LANGUAGE.into(),
            #[cfg(feature = "css")] Self::Css => tree_sitter_css::LANGUAGE.into(),
            #[cfg(feature = "bash")] Self::Bash => tree_sitter_bash::LANGUAGE.into(),
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

    let old_ts_tree = parser.parse(old_source, None).ok_or(FailedDiff::ParseFailed)?;
    let new_ts_tree = parser.parse(new_source, None).ok_or(FailedDiff::ParseFailed)?;

    let old_tree = build_tree(old_ts_tree.walk(), old_source);
    let new_tree = build_tree(new_ts_tree.walk(), new_source);

    let (old_ranges, new_ranges) = diff_trees(&old_tree, &new_tree, None, None, options)
        .ok_or(FailedDiff::GraphLimitExceeded)?;

    Ok(SyntaxDiffResult {
        old_ranges,
        new_ranges,
    })
}
