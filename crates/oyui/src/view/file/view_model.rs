//! Pre-computed view model for file rendering.

use crate::diff::FileDiff;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Identity of every input that determines the file view's row layout.
#[derive(PartialEq, Eq)]
struct LayoutKey {
    /// Pointer identity of the diff's content allocation.
    content_ptr: usize,
    /// Length of the new-side content in bytes.
    content_len: usize,
    /// Hunk count; split/join change the layout.
    hunks: usize,
    /// Whether context gaps collapse to separators.
    is_folded: bool,
    /// Collapsed context lines around each hunk.
    context_lines: usize,
}

impl LayoutKey {
    /// Reads the layout-identifying attributes of `diff` and its render mode.
    fn new(diff: &FileDiff, is_folded: bool, context_lines: usize) -> Self {
        Self {
            content_ptr: (&*diff.new_file_content).as_ptr() as usize,
            content_len: diff.new_file_content.len(),
            hunks: diff.hunks.len(),
            is_folded,
            context_lines,
        }
    }
}

/// Pre-computed derived state for file rendering.
///
/// Recomputed only when the diff or fold state changes, not every frame.
#[derive(Default)]
pub struct FileViewModel {
    row_counts: HashMap<PathBuf, usize>,
    line_mapping: HashMap<PathBuf, Vec<usize>>,
    hunk_starts: HashMap<PathBuf, Vec<usize>>,
    row_to_hunk: HashMap<PathBuf, Vec<Option<usize>>>,
    keys: HashMap<PathBuf, LayoutKey>,
}

impl FileViewModel {
    /// Recomputes the view model for a path.
    pub fn recompute(
        &mut self,
        path: &Path,
        diff: &FileDiff,
        is_folded: bool,
        context_lines: usize,
    ) {
        let mut line_map = Vec::new();
        let mut hunk_starts = Vec::new();
        let mut row_to_hunk = Vec::new();
        let mut current_new = 0;
        let mut visual_row_idx = 0;
        let new_lines_len = diff.new_file_content.lines().count();

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(context_lines);

            if is_folded && current_new < context_start {
                line_map.push(current_new);
                row_to_hunk.push(None);
                current_new = context_start;
                visual_row_idx += 1;
            }

            while current_new < hunk_new_start && current_new < new_lines_len {
                line_map.push(current_new);
                row_to_hunk.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }

            let mut recorded_hunk_start = false;
            for diff_line in &hunk.lines {
                if !recorded_hunk_start {
                    hunk_starts.push(visual_row_idx);
                    recorded_hunk_start = true;
                }

                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        line_map.push(current_new);
                        row_to_hunk.push(Some(i));
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Deletion { .. } => {
                        line_map.push(current_new);
                        row_to_hunk.push(Some(i));
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                        line_map.push(current_new);
                        row_to_hunk.push(Some(i));
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                }
            }

            if is_folded {
                let next_hunk_start = diff
                    .hunks
                    .get(i + 1)
                    .map(|h| h.after_lines.start)
                    .unwrap_or(new_lines_len);
                let context_end = current_new
                    .saturating_add(context_lines)
                    .min(next_hunk_start);

                while current_new < context_end && current_new < new_lines_len {
                    line_map.push(current_new);
                    row_to_hunk.push(None);
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        if !is_folded {
            while current_new < new_lines_len {
                line_map.push(current_new);
                row_to_hunk.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }
        } else if current_new < new_lines_len {
            line_map.push(current_new);
            row_to_hunk.push(None);
        }

        let total_rows = line_map.len();
        self.row_counts.insert(path.to_path_buf(), total_rows);
        self.line_mapping.insert(path.to_path_buf(), line_map);
        self.hunk_starts.insert(path.to_path_buf(), hunk_starts);
        self.row_to_hunk.insert(path.to_path_buf(), row_to_hunk);
        self.keys.insert(
            path.to_path_buf(),
            LayoutKey::new(diff, is_folded, context_lines),
        );
    }

    /// True when `path`'s layout metadata matches `diff` and its render mode.
    pub fn is_fresh(
        &self,
        path: &Path,
        diff: &FileDiff,
        is_folded: bool,
        context_lines: usize,
    ) -> bool {
        self.keys.get(path) == Some(&LayoutKey::new(diff, is_folded, context_lines))
    }

    /// Returns the row count for a path.
    pub fn row_count(&self, path: &Path) -> usize {
        self.row_counts.get(path).copied().unwrap_or(0)
    }

    /// Returns the line mapping for a path.
    pub fn line_mapping(&self, path: &Path) -> Option<&Vec<usize>> {
        self.line_mapping.get(path)
    }

    /// Returns the hunk starts for a path.
    pub fn hunk_starts(&self, path: &Path) -> Option<&Vec<usize>> {
        self.hunk_starts.get(path)
    }

    /// Returns the row-to-hunk mapping for a path.
    pub fn row_to_hunk(&self, path: &Path) -> Option<&Vec<Option<usize>>> {
        self.row_to_hunk.get(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, Hunk, LineSelections};
    use crate::terminal_colors::TerminalColorMode;
    use crate::theme::ansi_default_theme;
    use crate::view::file::render::rows::RowBuilder;

    /// Two hunks far enough apart that folding inserts a gap separator.
    fn two_hunk_diff() -> FileDiff {
        let new_content = (0..40)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let old_content = (0..38)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let hunk = |new_start: usize| Hunk {
            before_lines: new_start..new_start + 3,
            after_lines: new_start..new_start + 5,
            lines: vec![
                DiffLine::Context {
                    new_line_idx: new_start,
                    old_line_idx: new_start,
                },
                DiffLine::Addition {
                    new_line_idx: new_start + 1,
                    inline_highlights: Vec::new(),
                },
                DiffLine::Context {
                    new_line_idx: new_start + 2,
                    old_line_idx: new_start + 1,
                },
                DiffLine::Addition {
                    new_line_idx: new_start + 3,
                    inline_highlights: Vec::new(),
                },
                DiffLine::Context {
                    new_line_idx: new_start + 4,
                    old_line_idx: new_start + 2,
                },
            ],
            marker: Default::default(),
        };
        FileDiff {
            old_file_content: old_content.into(),
            new_file_content: new_content.into(),
            hunks: vec![hunk(0), hunk(30)],
            line_selections: LineSelections::default(),
        }
    }

    /// Navigation targets (`hunk_starts`) must point at the first rendered
    /// row of each hunk (`row_to_hunk`) — regression: folded gap separators
    /// were not counted as visual rows, shifting every later start.
    #[test]
    fn hunk_starts_point_at_first_row_of_each_hunk() {
        let diff = two_hunk_diff();
        let mut model = FileViewModel::default();
        let path = Path::new("a.txt");
        model.recompute(path, &diff, true, 4);

        let starts = model.hunk_starts(path).expect("starts computed");
        let mapping = model.row_to_hunk(path).expect("mapping computed");
        assert_eq!(starts.len(), diff.hunks.len());
        for (hunk_idx, start) in starts.iter().enumerate() {
            let expected = mapping
                .iter()
                .position(|h| *h == Some(hunk_idx))
                .expect("hunk appears in mapping");
            assert_eq!(
                *start, expected,
                "hunk {hunk_idx} start must be its first mapped row"
            );
        }
    }

    /// Split/join mutate the diff in place between draws; the layout key must
    /// detect it or `space`/`t`/`s` map the cursor through a stale layout.
    #[test]
    fn layout_key_invalidates_on_layout_changes() {
        let diff = two_hunk_diff();
        let mut model = FileViewModel::default();
        let path = Path::new("a.txt");
        model.recompute(path, &diff, true, 4);

        assert!(
            model.is_fresh(path, &diff, true, 4),
            "unchanged input is fresh"
        );
        assert!(
            !model.is_fresh(path, &diff, false, 4),
            "fold mode changes layout"
        );
        assert!(
            !model.is_fresh(path, &diff, true, 2),
            "context size changes layout"
        );

        let mut split = two_hunk_diff();
        split.hunks.push(Hunk {
            before_lines: 20..21,
            after_lines: 20..22,
            lines: vec![DiffLine::Context {
                new_line_idx: 20,
                old_line_idx: 20,
            }],
            marker: Default::default(),
        });
        assert!(
            !model.is_fresh(path, &split, true, 4),
            "split/join invalidates"
        );

        let mut replaced = two_hunk_diff();
        replaced.new_file_content = "brand\nnew\ncontent".into();
        assert!(
            !model.is_fresh(path, &replaced, true, 4),
            "new diff invalidates"
        );
    }

    /// The view model and the render row builder must count the same rows —
    /// the cursor highlights render rows while staging keys index this model.
    #[test]
    fn view_model_count_matches_render_builder() {
        let diff = two_hunk_diff();
        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();

        for folded in [false, true] {
            let mut model = FileViewModel::default();
            let path = Path::new("a.txt");
            model.recompute(path, &diff, folded, 4);

            let builder = RowBuilder {
                diff: &diff,
                old_lines: &new_lines,
                new_lines: &new_lines,
                syntax_opt: None,
                theme: &theme,
                hscroll: 0,
                area_width: 80,
                use_gradient: false,
                context_lines: 4,
                is_folded: folded,
                default_staged: true,
                selected_row_idx: 0,
            };
            let total = builder.build(None, &mut Vec::new());

            assert_eq!(
                model.row_to_hunk(path).expect("mapping").len(),
                total,
                "mapping rows must equal rendered rows (folded={folded})"
            );
            assert_eq!(model.row_count(path), total);
        }
    }
}
