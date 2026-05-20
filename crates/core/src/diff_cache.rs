use crate::cache_map::CacheMap;
use crate::diff::FileDiff;
use syntect::highlighting::Style as SyntectStyle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
}

/// All lazily-computed diff data, keyed by file path.
/// Lives outside the tree so the tree stays a pure structural model.
#[derive(Debug, Default)]
pub struct DiffCache {
    pub stats: CacheMap<DiffStats>,
    pub diffs: CacheMap<FileDiff>,
    pub syntax: CacheMap<Vec<Vec<(SyntectStyle, String)>>>,
}
