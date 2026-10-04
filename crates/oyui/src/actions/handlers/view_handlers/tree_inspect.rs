//! Read-only introspection of the file tree and cached diffs.

use std::path::PathBuf;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::diff::DiffResult;
use crate::view::tree::TreeRow;

impl ViewTreeInspectActionsHandler for AppActionsHandler {
    fn files(&self) -> String {
        self.tree
            .read()
            .files()
            .map(|f| f.path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn path_at(&self, i: u32) -> String {
        self.row_at(i)
            .map(|r| r.path.display().to_string())
            .unwrap_or_default()
    }

    fn is_dir_at(&self, i: u32) -> bool {
        self.row_at(i).map(|r| r.is_dir).unwrap_or(false)
    }

    fn staging_at(&self, i: u32) -> String {
        staging_name(self.row_at(i).map(|r| r.staging_state))
    }

    fn file_diff_text(&self, path: String) -> String {
        let path = PathBuf::from(path);
        match self.cache.diffs.get(&path).as_deref() {
            Some(DiffResult::Text(diff)) => super::file_handler::file_inspect::unified_diff(diff),
            _ => String::new(),
        }
    }

    fn file_stats(&self, path: String) -> String {
        let path = PathBuf::from(path);
        super::file_handler::file_inspect::format_stats(self.cache.stats.get(&path).as_deref())
    }

    fn file_staging(&self, path: String) -> String {
        staging_name(self.tree.read().get_file_state(&PathBuf::from(path)))
    }
}

impl AppActionsHandler {
    /// Clones the flat tree row at `index`, if any.
    fn row_at(&self, index: u32) -> Option<TreeRow> {
        let tree = self.tree.read();
        let mut ui = self.ui.lock();
        ui.tree_view
            .flat_rows(&tree, &self.cache)
            .get(index as usize)
            .cloned()
    }
}

/// Renders a staging state as a stable lowercase token.
fn staging_name(state: Option<crate::tree::StagingState>) -> String {
    match state {
        Some(crate::tree::StagingState::Staged) => "staged",
        Some(crate::tree::StagingState::PartiallyStaged) => "partial",
        Some(crate::tree::StagingState::Unstaged) => "unstaged",
        None => "none",
    }
    .to_string()
}
