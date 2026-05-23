use crate::commons::cache_map::CacheMap;
use crate::diff::{DiffResult, DiffStats};
use syntect::highlighting::Style as SyntectStyle;

/// All lazily-computed diff data, keyed by file path.
/// Lives outside the tree so the tree stays a pure structural model.
#[derive(Debug, Default)]
pub struct DiffCache {
    pub stats: CacheMap<DiffStats>,
    pub diffs: CacheMap<DiffResult>,
    pub syntax: CacheMap<Vec<Vec<(SyntectStyle, String)>>>,
}
