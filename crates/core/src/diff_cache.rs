use crate::diff::FileDiff;
use crate::lazy::Lazy;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffStats {
    pub insertions: usize,
    pub deletions: usize,
}

/// All lazily-computed diff data, keyed by file path.
/// Lives outside the tree so the tree stays a pure structural model.
#[derive(Debug, Default)]
pub struct DiffCache {
    pub stats: HashMap<PathBuf, Lazy<DiffStats>>,
    pub diffs: HashMap<PathBuf, Lazy<FileDiff>>,
}

impl DiffCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_stats(&self, path: &PathBuf) -> &Lazy<DiffStats> {
        self.stats.get(path).unwrap_or(&Lazy::Unstarted)
    }

    pub fn set_stats(&mut self, path: PathBuf, stats: DiffStats) {
        self.stats.insert(path, Lazy::Ready(stats));
    }

    pub fn invalidate_stats(&mut self, path: &PathBuf) {
        if let Some(entry) = self.stats.get_mut(path) {
            entry.invalidate();
        }
    }

    pub fn get_diff(&self, path: &PathBuf) -> &Lazy<FileDiff> {
        self.diffs.get(path).unwrap_or(&Lazy::Unstarted)
    }

    pub fn set_diff(&mut self, path: PathBuf, diff: FileDiff) {
        self.diffs.insert(path, Lazy::Ready(diff));
    }

    pub fn mark_started(&mut self, path: PathBuf) {
        self.diffs.entry(path).or_insert(Lazy::Started);
    }
}
