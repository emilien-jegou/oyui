use crate::commons::cache_map::CacheMap;
use crate::diff::{DiffResult, DiffStats};
use std::ops::Deref;
use std::sync::Arc;
use syntect::highlighting::Style as SyntectStyle;

/// All lazily-computed diff data, keyed by file path.
/// Lives outside the tree so the tree stays a pure structural model.
#[derive(Clone, Default)]
pub struct DiffCache(Arc<DiffCacheInner>);

impl Deref for DiffCache {
    type Target = DiffCacheInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Default)]
pub struct DiffCacheInner {
    pub stats: CacheMap<DiffStats>,
    pub diffs: CacheMap<DiffResult>,
    pub syntax: CacheMap<Vec<Vec<(SyntectStyle, String)>>>,
}
