use crate::commons::cache_map::CacheMap;
use crate::diff::{DiffResult, DiffStats, LineRanges};
use std::ops::Deref;
use std::sync::Arc;
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

/// Per-side line offsets of a diff, published once by the diff listener.
///
/// Rendering used to call `content.lines()` and collect both sides into a
/// `Vec<&str>` on every frame; with this cached the renderer indexes a line
/// directly, so a frame no longer scales with the size of the file.
#[derive(Clone, Default)]
pub struct LineIndex {
    pub old: Arc<LineRanges>,
    pub new: Arc<LineRanges>,
    /// Allocation identity of each side's content, as in `LayoutKey`.
    ///
    /// Staging and undo can replace a cached diff without the diff listener
    /// running, so the renderer must confirm the index still describes the
    /// content it is about to index.
    pub old_ptr: usize,
    pub new_ptr: usize,
}

impl LineIndex {
    /// Builds an index for a diff's two sides.
    pub fn of(diff: &crate::diff::FileDiff) -> Self {
        Self {
            old: Arc::new(crate::diff::line_ranges(&diff.old_file_content)),
            new: Arc::new(crate::diff::line_ranges(&diff.new_file_content)),
            old_ptr: Arc::as_ptr(&diff.old_file_content).cast::<u8>() as usize,
            new_ptr: Arc::as_ptr(&diff.new_file_content).cast::<u8>() as usize,
        }
    }

    /// True when this index still describes `diff`'s content.
    pub fn matches(&self, diff: &crate::diff::FileDiff) -> bool {
        self.old_ptr == Arc::as_ptr(&diff.old_file_content).cast::<u8>() as usize
            && self.new_ptr == Arc::as_ptr(&diff.new_file_content).cast::<u8>() as usize
    }
}

#[derive(Clone, Default)]
pub struct DiffCacheInner {
    pub stats: CacheMap<DiffStats>,
    pub diffs: CacheMap<DiffResult>,
    pub syntax: CacheMap<Vec<Vec<(SyntectStyle, String)>>>,
    pub line_index: CacheMap<LineIndex>,
    /// Widest line of each side, in display columns.
    ///
    /// Only horizontal scrolling needs it, and it cannot change without the
    /// content changing, so the diff listener derives it once instead of every
    /// scroll keypress rescanning the whole file.
    pub line_widths: CacheMap<LineWidths>,
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
