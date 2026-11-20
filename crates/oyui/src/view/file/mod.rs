pub mod mapping;
pub mod render;
pub mod utils;
pub mod view_model;

use crate::view::file::view_model::{ConflictRegions, FileViewModel};
use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct FileViewData {
    pub scroll_states: HashMap<PathBuf, TableState>,
    pub hscroll_states: HashMap<PathBuf, usize>,
    pub current_path: Option<PathBuf>,
    pub scrolloff: usize,
    pub is_folded: bool,
    pub context_lines: usize,
    pub last_height: usize,
    pub last_width: usize,
    pub use_gradient: bool,
    /// Foldable conflict regions, refreshed from the resolver before drawing.
    pub conflict_regions: ConflictRegions,
    view_model: FileViewModel,
    view_model_dirty: bool,
}

impl FileViewData {
    pub fn new(use_gradient: bool) -> Self {
        Self {
            scroll_states: HashMap::new(),
            hscroll_states: HashMap::new(),
            current_path: None,
            scrolloff: 0,
            is_folded: true,
            context_lines: 4,
            last_height: 0,
            last_width: 0,
            use_gradient,
            conflict_regions: ConflictRegions::default(),
            view_model: FileViewModel::default(),
            view_model_dirty: true,
        }
    }

    /// Marks the view model as needing recomputation.
    pub fn mark_dirty(&mut self) {
        self.view_model_dirty = true;
    }

    /// Returns the row count for a path kept fresh by render-time recomputes.
    pub fn row_count(&mut self, path: &PathBuf) -> usize {
        self.view_model.row_count(path)
    }

    /// Returns the line mapping for a path.
    pub fn line_mapping(&self, path: &PathBuf) -> Option<&Vec<usize>> {
        self.view_model.line_mapping(path)
    }

    /// Returns the hunk starts for a path.
    pub fn hunk_starts(&self, path: &PathBuf) -> Option<&Vec<usize>> {
        self.view_model.hunk_starts(path)
    }

    /// Returns the row-to-hunk mapping for a path.
    pub fn row_to_hunk(&self, path: &PathBuf) -> Option<&Vec<Option<usize>>> {
        self.view_model.row_to_hunk(path)
    }

    /// Row count for the current path, taken from the view model.
    ///
    /// The count pass in the row builder is only a fallback for a path the
    /// model has never seen, so a steady-state frame reads a cached number
    /// instead of walking every hunk again.
    pub fn view_model_row_count(&self, path: &PathBuf) -> usize {
        self.view_model.row_count(path)
    }

    /// Refreshes the layout metadata for the current path when dirty or stale.
    ///
    /// The layout key also catches diff mutations (split/join) that happen
    /// between draws — e.g. inside one batch of keys handled per frame.
    pub fn recompute_view_model(&mut self, diff: &crate::diff::FileDiff) {
        if let Some(path) = &self.current_path {
            if self.view_model_dirty
                || !self.view_model.is_fresh(
                    path,
                    diff,
                    self.is_folded,
                    self.context_lines,
                    &self.conflict_regions,
                )
            {
                self.view_model.recompute(
                    path,
                    diff,
                    self.is_folded,
                    self.context_lines,
                    &self.conflict_regions,
                );
                self.view_model_dirty = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, FileDiff, Hunk};
    use std::sync::Arc;

    fn one_line_diff() -> FileDiff {
        FileDiff {
            old_file_content: Arc::from(""),
            new_file_content: Arc::from("x"),
            hunks: vec![Hunk {
                before_lines: 0..0,
                after_lines: 0..1,
                lines: vec![DiffLine::Addition {
                    new_line_idx: 0,
                    inline_highlights: Vec::new(),
                }],
                marker: Default::default(),
            }],
            line_selections: Default::default(),
        }
    }

    /// Opening a second file must refresh navigation data even though the
    /// dirty flag (set only by fold toggles) stays false across the switch.
    #[test]
    fn file_view_model_follows_current_path() {
        let mut view = FileViewData::new(false);

        view.current_path = Some(PathBuf::from("a"));
        view.recompute_view_model(&one_line_diff());
        assert_eq!(view.row_count(&PathBuf::from("a")), 1);

        view.current_path = Some(PathBuf::from("b"));
        view.recompute_view_model(&one_line_diff());
        assert_eq!(
            view.row_count(&PathBuf::from("b")),
            1,
            "opening a file must refresh navigation data for the new path"
        );
    }
}
