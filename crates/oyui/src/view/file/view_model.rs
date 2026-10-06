//! Pre-computed view model for file rendering.

/// New-side line layout of one conflict: where each side lives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConflictSides {
    /// Lines of the ours section (excluding markers).
    pub ours: Range<usize>,
    /// Lines of the theirs section (excluding markers).
    pub theirs: Range<usize>,
    /// New-side line index of the `=======` separator.
    pub sep: usize,
}

use crate::diff::FileDiff;
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Foldable conflict regions over the new-side line indices.
///
/// A folded conflict renders a full-width header framing the chosen side's
/// lines plus a full-width footer, so the choice stays visible and editable;
/// markers, base and the losing side are hidden. Headers and footers take no
/// file line, just the whole width (like fold separators). This is
/// display-only, so the underlying diff (and its staging selections) are
/// untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConflictRegions {
    /// New-content line spans of each conflict.
    pub ranges: Vec<std::ops::Range<usize>>,
    /// Whether each conflict is collapsed.
    pub folded: Vec<bool>,
    /// Header text per conflict (shown when folded).
    pub headers: Vec<String>,
    /// Footer text per conflict (shown when folded).
    pub footers: Vec<String>,
    /// Chosen side per conflict; drives which lines stay visible when folded.
    pub choices: Vec<Option<crate::diff::Side>>,
    /// Visible line ranges per conflict when folded (the kept side's lines).
    pub kept: Vec<Vec<std::ops::Range<usize>>>,
    /// Per-conflict side layout (ours/theirs ranges + separator index).
    pub sides: Vec<ConflictSides>,
}

impl ConflictRegions {
    /// True when there is nothing to fold.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// For a new-content line in a folded conflict: `(index, is_first_line)`.
    /// Kept (chosen-side) lines return `None` so they render as normal rows.
    pub fn folded_span(&self, line: usize) -> Option<(usize, bool)> {
        self.fold_action(line).and_then(|(i, header, visible)| {
            if visible { None } else { Some((i, header)) }
        })
    }

    /// Fold status of a conflict line: `(index, is_header, visible_as_normal_row)`.
    pub fn fold_action(&self, line: usize) -> Option<(usize, bool, bool)> {
        self.ranges.iter().enumerate().find_map(|(i, r)| {
            if !(self.folded.get(i).copied().unwrap_or(false) && r.contains(&line)) {
                return None;
            }
            if line == r.start {
                return Some((i, true, false));
            }
            let visible = self
                .kept
                .get(i)
                .map(|ranges| ranges.iter().any(|k| k.contains(&line)))
                .unwrap_or(false);
            Some((i, false, visible))
        })
    }

    /// Which side space would select for `line` in conflict `index`.
    ///
    /// Lines above the `=======` separator (markers and ours) map to ours,
    /// the separator itself and everything below map to theirs, the base
    /// block maps to ours.
    pub fn side_at_line(
        &self,
        index: usize,
        line: usize,
    ) -> Option<crate::diff::Side> {
        use crate::diff::Side;
        let range = self.ranges.get(index)?;
        if !range.contains(&line) {
            return None;
        }
        let sides = self.sides.get(index)?;
        if line < sides.ours.end {
            Some(Side::Ours)
        } else if line >= sides.sep {
            Some(Side::Theirs)
        } else {
            Some(Side::Ours)
        }
    }

    /// True when `line` lies in any conflict range, folded or not.
    ///
    /// Staging display is neutralized there: conflict hunks are not
    /// stageable, so their colors must never reflect staging state.
    pub fn covers(&self, line: usize) -> bool {
        self.ranges.iter().any(|r| r.contains(&line))
    }

    /// Header text for conflict `i`.
    pub fn header(&self, i: usize) -> &str {
        self.headers.get(i).map(String::as_str).unwrap_or("")
    }

    /// Footer text for conflict `i`.
    pub fn footer(&self, i: usize) -> &str {
        self.footers.get(i).map(String::as_str).unwrap_or("")
    }
}

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
    /// Folded conflict flags; folding changes the row layout.
    conflict_folded: Vec<bool>,
    /// Chosen sides; the kept lines stay visible when folded.
    conflict_choices: Vec<Option<crate::diff::Side>>,
}

impl LayoutKey {
    /// Reads the layout-identifying attributes of `diff` and its render mode.
    fn new(
        diff: &FileDiff,
        is_folded: bool,
        context_lines: usize,
        regions: &ConflictRegions,
    ) -> Self {
        Self {
            content_ptr: (&*diff.new_file_content).as_ptr() as usize,
            content_len: diff.new_file_content.len(),
            hunks: diff.hunks.len(),
            is_folded,
            context_lines,
            conflict_folded: regions.folded.clone(),
            conflict_choices: regions.choices.clone(),
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
        regions: &ConflictRegions,
    ) {
        let mut line_map = Vec::new();
        let mut hunk_starts = Vec::new();
        let mut row_to_hunk = Vec::new();
        let mut current_new = 0;
        let mut visual_row_idx = 0;
        let new_lines_len = diff.new_file_content.lines().count();
        // Conflict whose header was emitted and whose footer is still
        // pending (emitted once the scan passes its range end). Hoisted
        // across hunks in case a conflict ever spans a hunk boundary.
        let mut open_frame: Option<usize> = None;

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
                // Folded conflicts hide their lines even when the diff leaves
                // them as inter-hunk context (no row, no mapping entry).
                if regions.folded_span(current_new).is_some() {
                    current_new += 1;
                    continue;
                }
                line_map.push(current_new);
                row_to_hunk.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }

            let mut recorded_hunk_start = false;
            for diff_line in &hunk.lines {
                // A folded conflict shows its header; hidden lines are skipped,
                // kept (chosen-side) lines fall through to normal rendering.
                if let Some((ci, is_first)) = folded_line(diff_line, current_new, regions) {
                    if is_first {
                        if !recorded_hunk_start {
                            hunk_starts.push(visual_row_idx);
                            recorded_hunk_start = true;
                        }
                        // A frame never reopens inside itself.
                        if open_frame != Some(ci) {
                            line_map.push(regions.ranges[ci].start);
                            row_to_hunk.push(Some(i));
                            visual_row_idx += 1;
                            open_frame = Some(ci);
                        }
                    }
                    if let crate::diff::DiffLine::Context { new_line_idx, .. }
                    | crate::diff::DiffLine::Addition { new_line_idx, .. } = diff_line
                    {
                        current_new = *new_line_idx + 1;
                    }
                    continue;
                }

                // Past the framed range: close it with a footer row. The
                // footer maps to the range start so space on it unfolds.
                if let Some(ci) = open_frame {
                    let past_end = match diff_line {
                        crate::diff::DiffLine::Context { new_line_idx, .. }
                        | crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                            *new_line_idx >= regions.ranges[ci].end
                        }
                        crate::diff::DiffLine::Deletion { .. } => {
                            current_new >= regions.ranges[ci].end
                        }
                    };
                    if past_end {
                        if !recorded_hunk_start {
                            hunk_starts.push(visual_row_idx);
                            recorded_hunk_start = true;
                        }
                        line_map.push(regions.ranges[ci].start);
                        row_to_hunk.push(Some(i));
                        visual_row_idx += 1;
                        open_frame = None;
                    }
                }

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

            // End of hunk: close the frame when its range was fully scanned.
            if let Some(ci) = open_frame {
                if current_new >= regions.ranges[ci].end {
                    if !recorded_hunk_start {
                        hunk_starts.push(visual_row_idx);
                        recorded_hunk_start = true;
                    }
                    line_map.push(regions.ranges[ci].start);
                    row_to_hunk.push(Some(i));
                    visual_row_idx += 1;
                    open_frame = None;
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
                    // Same folded-conflict hiding as the pre-hunk gap above.
                    if regions.folded_span(current_new).is_some() {
                        current_new += 1;
                        continue;
                    }
                    line_map.push(current_new);
                    row_to_hunk.push(None);
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        if !is_folded {
            while current_new < new_lines_len {
                // Folded frames hide their lines even in the tail.
                if regions.folded_span(current_new).is_some() {
                    current_new += 1;
                    continue;
                }
                line_map.push(current_new);
                row_to_hunk.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }
        } else if current_new < new_lines_len {
            line_map.push(current_new);
            row_to_hunk.push(None);
        }

        // Trailing flush: a frame ending at the very end of the file.
        if let Some(ci) = open_frame.take() {
            line_map.push(regions.ranges[ci].start);
            row_to_hunk.push(diff.hunks.len().checked_sub(1));
        }

        let total_rows = line_map.len();
        self.row_counts.insert(path.to_path_buf(), total_rows);
        self.line_mapping.insert(path.to_path_buf(), line_map);
        self.hunk_starts.insert(path.to_path_buf(), hunk_starts);
        self.row_to_hunk.insert(path.to_path_buf(), row_to_hunk);
        self.keys.insert(
            path.to_path_buf(),
            LayoutKey::new(diff, is_folded, context_lines, regions),
        );
    }

    /// True when `path`'s layout metadata matches `diff` and its render mode.
    pub fn is_fresh(
        &self,
        path: &Path,
        diff: &FileDiff,
        is_folded: bool,
        context_lines: usize,
        regions: &ConflictRegions,
    ) -> bool {
        self.keys.get(path) == Some(&LayoutKey::new(diff, is_folded, context_lines, regions))
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

/// For a diff line inside a folded conflict: `(conflict index, is_first_line)`.
fn folded_line(
    line: &crate::diff::DiffLine,
    current_new: usize,
    regions: &ConflictRegions,
) -> Option<(usize, bool)> {
    match line {
        crate::diff::DiffLine::Context { new_line_idx, .. }
        | crate::diff::DiffLine::Addition { new_line_idx, .. } => {
            regions.folded_span(*new_line_idx)
        }
        crate::diff::DiffLine::Deletion { .. } => regions.folded_span(current_new),
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
        model.recompute(path, &diff, true, 4, &ConflictRegions::default());

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
        model.recompute(path, &diff, true, 4, &ConflictRegions::default());

        assert!(
            model.is_fresh(path, &diff, true, 4, &ConflictRegions::default()),
            "unchanged input is fresh"
        );
        assert!(
            !model.is_fresh(path, &diff, false, 4, &ConflictRegions::default()),
            "fold mode changes layout"
        );
        assert!(
            !model.is_fresh(path, &diff, true, 2, &ConflictRegions::default()),
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
            !model.is_fresh(path, &split, true, 4, &ConflictRegions::default()),
            "split/join invalidates"
        );

        let mut replaced = two_hunk_diff();
        replaced.new_file_content = "brand\nnew\ncontent".into();
        assert!(
            !model.is_fresh(path, &replaced, true, 4, &ConflictRegions::default()),
            "new diff invalidates"
        );
    }

    /// A folded conflict renders as header + footer framing its kept lines in
    /// both the view model and the render builder, or navigation and
    /// rendering desync.
    #[test]
    fn folded_conflict_count_matches_render_builder() {
        let diff = FileDiff {
            old_file_content: std::sync::Arc::from("a\nbase\nb"),
            new_file_content: std::sync::Arc::from(
                "a\n<<<<<<< ours\none\n=======\ntwo\n>>>>>>> theirs\nb",
            ),
            hunks: vec![Hunk {
                before_lines: 0..3,
                after_lines: 0..7,
                lines: vec![
                    DiffLine::Context {
                        new_line_idx: 0,
                        old_line_idx: 0,
                    },
                    DiffLine::Addition {
                        new_line_idx: 1,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 2,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 3,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 4,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 5,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Context {
                        new_line_idx: 6,
                        old_line_idx: 2,
                    },
                ],
                marker: Default::default(),
            }],
            line_selections: LineSelections::default(),
        };

        let regions = ConflictRegions {
            ranges: vec![1..6],
            folded: vec![true],
            headers: vec!["ours".to_string()],
            footers: vec!["".to_string()],
            choices: vec![None],
            kept: vec![Vec::new()],
            sides: vec![ConflictSides {
                ours: 2..3,
                theirs: 4..5,
                sep: 3,
            }],
        };

        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();

        let mut model = FileViewModel::default();
        let path = Path::new("a.txt");
        model.recompute(path, &diff, false, 4, &regions);

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
            is_folded: false,
            default_staged: true,
            selected_row_idx: 0,
            regions: &regions,
            preview: None,
        };
        let total = builder.build(None, &mut Vec::new());

        assert_eq!(model.row_count(path), total);
        assert_eq!(total, 4, "a folded conflict renders as header + footer");
    }

    /// Base content the diff leaves as inter-hunk context must not leak into
    /// a folded frame: the base line matches base, so imara can split the
    /// marker block across two hunks with the base line as context between
    /// them. Folded, only header + kept + footer may render.
    #[test]
    fn folded_conflict_hides_inter_hunk_base_context() {
        let diff = FileDiff {
            old_file_content: std::sync::Arc::from("a\nbase\nb"),
            new_file_content: std::sync::Arc::from(
                "a\n<<<<<<< ours\none\n||||||| base\nbase\n=======\ntwo\n>>>>>>> theirs\nb",
            ),
            hunks: vec![
                Hunk {
                    before_lines: 0..1,
                    after_lines: 0..4,
                    lines: vec![
                        DiffLine::Context {
                            new_line_idx: 0,
                            old_line_idx: 0,
                        },
                        DiffLine::Addition {
                            new_line_idx: 1,
                            inline_highlights: Vec::new(),
                        },
                        DiffLine::Addition {
                            new_line_idx: 2,
                            inline_highlights: Vec::new(),
                        },
                        DiffLine::Addition {
                            new_line_idx: 3,
                            inline_highlights: Vec::new(),
                        },
                    ],
                    marker: Default::default(),
                },
                Hunk {
                    before_lines: 2..3,
                    after_lines: 5..9,
                    lines: vec![
                        DiffLine::Addition {
                            new_line_idx: 5,
                            inline_highlights: Vec::new(),
                        },
                        DiffLine::Addition {
                            new_line_idx: 6,
                            inline_highlights: Vec::new(),
                        },
                        DiffLine::Addition {
                            new_line_idx: 7,
                            inline_highlights: Vec::new(),
                        },
                        DiffLine::Context {
                            new_line_idx: 8,
                            old_line_idx: 2,
                        },
                    ],
                    marker: Default::default(),
                },
            ],
            line_selections: LineSelections::default(),
        };

        // Conflict spans new lines 1..8; line 4 (`base`) is inter-hunk
        // context, not part of either hunk.
        let regions = ConflictRegions {
            ranges: vec![1..8],
            folded: vec![true],
            headers: vec!["theirs".to_string()],
            footers: vec!["".to_string()],
            choices: vec![Some(crate::diff::Side::Theirs)],
            kept: vec![vec![6..7]],
            sides: vec![ConflictSides {
                ours: 2..3,
                theirs: 6..7,
                sep: 5,
            }],
        };

        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();

        let mut model = FileViewModel::default();
        let path = Path::new("a.txt");
        model.recompute(path, &diff, false, 4, &regions);

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
            is_folded: false,
            default_staged: true,
            selected_row_idx: 0,
            regions: &regions,
            preview: None,
        };
        let total = builder.build(None, &mut Vec::new());

        assert_eq!(model.row_count(path), total);
        // a, header, kept `two`, footer, b — the inter-hunk `base` line is hidden.
        assert_eq!(total, 5);
        let mapping = model.line_mapping(path).expect("mapping");
        assert!(
            !mapping.contains(&4),
            "base context line must not map to any row"
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
            model.recompute(path, &diff, folded, 4, &ConflictRegions::default());

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
                regions: &ConflictRegions::default(),
                preview: None,
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
