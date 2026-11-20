//! Pre-computed view model for file rendering.

use crate::diff::Side;

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
    /// Everything the row model needs per conflict, in one place.
    pub views: Vec<ConflictView>,
}

/// Everything the row model needs for one conflict, in one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictView {
    /// New-content line span of the conflict.
    pub range: std::ops::Range<usize>,
    /// Whether the conflict is collapsed.
    pub folded: bool,
    /// Header text (shown when folded).
    pub header: String,
    /// Footer text (shown when folded).
    pub footer: String,
    /// Chosen side; drives what stays visible when folded.
    pub choice: Option<crate::diff::Side>,
    /// New-side line indices of the kept (chosen) rows.
    pub kept: Vec<std::ops::Range<usize>>,
    /// Side layout inside the conflict.
    pub sides: ConflictSides,
}

impl ConflictRegions {
    /// Everything the row model needs for conflict `i`.
    pub fn get(&self, i: usize) -> Option<&ConflictView> {
        self.views.get(i)
    }

    /// True when there is nothing to fold.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// For a new-content line in a folded conflict: `(index, is_first_line)`.
    /// Kept (chosen-side) lines return `None` so they render as normal rows.
    pub fn folded_span(&self, line: usize) -> Option<(usize, bool)> {
        self.fold_action(line).and_then(
            |(i, header, visible)| {
                if visible {
                    None
                } else {
                    Some((i, header))
                }
            },
        )
    }

    /// Fold status of a conflict line: `(index, is_header, visible_as_normal_row)`.
    pub fn fold_action(&self, line: usize) -> Option<(usize, bool, bool)> {
        self.views.iter().enumerate().find_map(|(i, v)| {
            if !(v.folded && v.range.contains(&line)) {
                return None;
            }
            if line == v.range.start {
                return Some((i, true, false));
            }
            let visible = v.kept.iter().any(|k| k.contains(&line));
            Some((i, false, visible))
        })
    }

    /// Which side space would select for `line` in conflict `index`.
    ///
    /// Lines above the `=======` separator (markers and ours) map to ours,
    /// the separator itself and everything below map to theirs, the base
    /// block maps to ours.
    pub fn side_at_line(&self, index: usize, line: usize) -> Option<crate::diff::Side> {
        let v = self.views.get(index)?;
        if !v.range.contains(&line) {
            return None;
        }
        if line < v.sides.ours.end {
            Some(Side::Ours)
        } else if line >= v.sides.sep {
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
        self.views.iter().any(|v| v.range.contains(&line))
    }

    /// Header text for conflict `i`.
    pub fn header(&self, i: usize) -> &str {
        self.views.get(i).map(|v| v.header.as_str()).unwrap_or("")
    }

    /// Footer text for conflict `i`.
    pub fn footer(&self, i: usize) -> &str {
        self.views.get(i).map(|v| v.footer.as_str()).unwrap_or("")
    }
}

/// Identity of every input that determines the file view's row layout.
#[derive(Debug, PartialEq, Eq)]
struct LayoutKey {
    /// The diff revision this layout was computed from.
    ///
    /// Revisions are assigned monotonically on construction and bumped by
    /// structural mutations, so a replaced diff can never masquerade as
    /// "unchanged" — the allocation-address check this replaces could, once
    /// a freed `Arc` had its address reused (finding 3.7).
    content_revision: u64,
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
            content_revision: diff.revision,
            content_len: diff.new_file_content.len(),
            hunks: diff.hunks.len(),
            is_folded,
            context_lines,
            conflict_folded: regions.views.iter().map(|v| v.folded).collect(),
            conflict_choices: regions.views.iter().map(|v| v.choice).collect(),
        }
    }
}

/// The layout data for one path.
#[derive(Debug)]
struct PathLayout {
    path: PathBuf,
    key: LayoutKey,
    row_count: usize,
    line_mapping: Vec<usize>,
    hunk_starts: Vec<usize>,
    row_to_hunk: Vec<Option<usize>>,
}

/// Pre-computed derived state for file rendering.
///
/// Recomputed only when the diff or fold state changes, not every frame.
///
/// Holds exactly one path's layout — the file in view. Every frame
/// recomputes the current path when its key goes stale, so switching files
/// replaces the previous entry: nothing accumulates per opened path, and a
/// rename, deletion, or tree rebuild cannot leave orphans behind (finding
/// 6.14).
#[derive(Default)]
pub struct FileViewModel {
    current: Option<PathLayout>,
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
        // One line count model for the whole crate: the renderer's
        // `LineRanges` (finding 6.15). It shares `str::lines()`' trailing
        // newline semantics but is what the rendered rows are built from,
        // so the model and the builder can no longer disagree at EOF.
        let new_lines_len = crate::diff::line_ranges(&diff.new_file_content).len();
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
                            line_map.push(regions.views[ci].range.start);
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
                            *new_line_idx >= regions.views[ci].range.end
                        }
                        crate::diff::DiffLine::Deletion { .. } => {
                            current_new >= regions.views[ci].range.end
                        }
                    };
                    if past_end {
                        if !recorded_hunk_start {
                            hunk_starts.push(visual_row_idx);
                            recorded_hunk_start = true;
                        }
                        line_map.push(regions.views[ci].range.start);
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
                if current_new >= regions.views[ci].range.end {
                    if !recorded_hunk_start {
                        hunk_starts.push(visual_row_idx);
                        recorded_hunk_start = true;
                    }
                    line_map.push(regions.views[ci].range.start);
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
            line_map.push(regions.views[ci].range.start);
            row_to_hunk.push(diff.hunks.len().checked_sub(1));
        }
        let total_rows = line_map.len();
        self.current = Some(PathLayout {
            path: path.to_path_buf(),
            key: LayoutKey::new(diff, is_folded, context_lines, regions),
            row_count: total_rows,
            line_mapping: line_map,
            hunk_starts: hunk_starts,
            row_to_hunk: row_to_hunk,
        });
    }

    /// The entry for `path`, when it is the one the model currently holds.
    fn layout(&self, path: &Path) -> Option<&PathLayout> {
        self.current.as_ref().filter(|entry| entry.path == path)
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
        self.layout(path).is_some_and(|entry| {
            entry.key == LayoutKey::new(diff, is_folded, context_lines, regions)
        })
    }

    /// Returns the row count for a path.
    pub fn row_count(&self, path: &Path) -> usize {
        self.layout(path).map_or(0, |entry| entry.row_count)
    }

    /// Returns the line mapping for a path.
    pub fn line_mapping(&self, path: &Path) -> Option<&Vec<usize>> {
        self.layout(path).map(|entry| &entry.line_mapping)
    }

    /// Returns the hunk starts for a path.
    pub fn hunk_starts(&self, path: &Path) -> Option<&Vec<usize>> {
        self.layout(path).map(|entry| &entry.hunk_starts)
    }

    /// Returns the row-to-hunk mapping for a path.
    pub fn row_to_hunk(&self, path: &Path) -> Option<&Vec<Option<usize>>> {
        self.layout(path).map(|entry| &entry.row_to_hunk)
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
            revision: 0,
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

    /// A replaced diff with identical geometry (same byte length, same hunk
    /// count) must still invalidate: the revision, not the allocation
    /// address, is the identity. The old pointer check read a replacement
    /// whose reused freed allocation landed at the same address as
    /// "unchanged" (finding 3.7).
    #[test]
    fn same_shape_with_replaced_content_invalidates() {
        let path = Path::new("a.txt");
        let diff = two_hunk_diff();
        let mut model = FileViewModel::default();
        model.recompute(path, &diff, true, 4, &ConflictRegions::default());

        // Same byte length ("l5" -> "L5"), same hunk count, different text
        // and revision.
        let mut lookalike = two_hunk_diff();
        lookalike.new_file_content = diff.new_file_content.replacen("l5", "L5", 1).into();
        assert_eq!(
            lookalike.new_file_content.len(),
            diff.new_file_content.len()
        );
        lookalike.revision = crate::diff::FileDiff::next_revision();

        assert!(
            !model.is_fresh(path, &lookalike, true, 4, &ConflictRegions::default()),
            "a same-shape replacement with a new revision must invalidate"
        );
    }

    /// The row model derives its line count from the shared `LineRanges`
    /// model the renderer builds its `LineAccess` on: a trailing newline
    /// must not manufacture a phantom last row, and CRLF must not change
    /// the count (finding 6.15).
    #[test]
    fn row_count_follows_the_render_line_ranges() {
        let path = Path::new("a.txt");
        for content in [
            "a\nb\nc",
            "a\nb\nc\n",
            "a\r\nb\r\nc",
            "a\r\nb\r\nc\r\n",
            "a\n\nb",
            "",
        ] {
            let diff = FileDiff {
                old_file_content: content.into(),
                new_file_content: content.into(),
                hunks: Vec::new(),
                line_selections: LineSelections::default(),
                revision: 0,
            };
            let mut model = FileViewModel::default();
            model.recompute(path, &diff, false, 4, &ConflictRegions::default());

            assert_eq!(
                model.row_count(path),
                crate::diff::line_ranges(content).len(),
                "rows must equal the rendered line count for {content:?}"
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
}
