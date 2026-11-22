use crate::commons::cache_map::CacheMap;
use crate::diff::{DiffResult, DiffStats, LineRanges};
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use syntect::highlighting::Style as SyntectStyle;

/// All lazily-computed diff data, keyed by file path.
/// Lives outside the tree so the tree stays a pure structural model.
#[derive(Clone, Default)]
pub struct DiffCache(Arc<DiffCacheInner>);

impl Deref for DiffCache {
    type Target = DiffCacheInner;

    fn deref(&self) -> &DiffCacheInner {
        &self.0
    }
}

impl DiffCache {
    /// Advances the selected-row generation used to discard stale hover
    /// preloads. This is intentionally independent of cache invalidation.
    pub fn next_preload_generation(&self) -> u64 {
        self.0.preload_generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn preload_generation(&self) -> u64 {
        self.0.preload_generation.load(Ordering::Relaxed)
    }
}

/// Per-side line offsets of a diff, published once by the diff listener.
///
/// Rendering used to call `content.lines()` and collect both sides into a
/// `Vec<&str>` on every frame; with this cached the renderer indexes a line
/// directly, so a frame no longer scales with the size of the file.
#[derive(Clone, Default)]
pub struct LineIndex {
    pub old: Arc<LineRanges>,
    pub new: Arc<LineRanges>,
    /// The diff revision this index was built for.
    ///
    /// Staging and undo can replace a cached diff without the diff listener
    /// running, so the renderer must confirm the index still describes the
    /// diff it is about to index. Revisions are monotonically assigned on
    /// construction, so a replaced diff can never read as "unchanged" —
    /// unlike the allocation-address check this replaces (finding 3.7).
    pub revision: u64,
}

impl LineIndex {
    /// Builds an index for a diff's two sides.
    pub fn of(diff: &crate::diff::FileDiff) -> Self {
        Self {
            old: Arc::new(crate::diff::line_ranges(&diff.old_file_content)),
            new: Arc::new(crate::diff::line_ranges(&diff.new_file_content)),
            revision: diff.revision,
        }
    }

    /// True when this index still describes `diff`.
    pub fn matches(&self, diff: &crate::diff::FileDiff) -> bool {
        self.revision == diff.revision
    }
}

#[derive(Clone, Default)]
pub struct DiffCacheInner {
    pub stats: CacheMap<DiffStats>,
    pub diffs: CacheMap<DiffResult>,
    pub syntax: CacheMap<SideTokens>,
    pub line_index: CacheMap<LineIndex>,
    /// Widest line of each side, in display columns.
    ///
    /// Only horizontal scrolling needs it, and it cannot change without the
    /// content changing, so the diff listener derives it once instead of every
    /// scroll keypress rescanning the whole file.
    pub line_widths: CacheMap<LineWidths>,
    /// Shared generation for debounced tree-hover preloads.
    preload_generation: Arc<AtomicU64>,
}

/// Syntax tokens for both sides of a diff, in line order.
///
/// Deletion rows index `old` (they show old-side content), additions and
/// context rows index `new`.
#[derive(Clone, Default)]
pub struct SideTokens {
    pub old: Arc<Vec<Vec<(SyntectStyle, String)>>>,
    pub new: Arc<Vec<Vec<(SyntectStyle, String)>>>,
}

/// Widest line of each side of a diff, in display columns.
#[derive(Clone, Copy, Default)]
pub struct LineWidths {
    pub old: usize,
    pub new: usize,
}

impl LineWidths {
    /// The widest of both sides.
    pub fn max(self) -> usize {
        self.old.max(self.new)
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, LineWidths};
    use crate::diff::FileDiff;

    fn diff_of(old: &str, new: &str) -> FileDiff {
        FileDiff {
            old_file_content: old.into(),
            new_file_content: new.into(),
            hunks: Vec::new(),
            line_selections: Default::default(),
            revision: FileDiff::next_revision(),
        }
    }

    /// An index must be rejected once the cached diff it described was
    /// replaced: staging and undo swap `DiffResult`s without the diff listener
    /// running, and indexing stale offsets would render the wrong lines.
    #[test]
    fn line_index_is_rejected_when_content_is_replaced() {
        let diff = diff_of("a\nb", "a\nB");
        let index = LineIndex::of(&diff);
        assert!(index.matches(&diff), "fresh index matches its diff");

        let replaced = diff_of("a\nb", "completely\ndifferent\ncontent");
        assert!(
            !index.matches(&replaced),
            "an index built for other content must not be reused"
        );

        // Same length, different content: identity, not size, is what counts.
        let same_len = diff_of("a\nb", "x\ny");
        assert!(!index.matches(&same_len));
    }

    #[test]
    fn line_widths_report_the_widest_side() {
        let widths = LineWidths { old: 3, new: 12 };
        assert_eq!(widths.max(), 12);
    }
}
