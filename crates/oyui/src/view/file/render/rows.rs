//! File view row building: counts all visual rows, renders only a window.

use std::ops::Range;

use ratatui::widgets::Row;
use syntect::highlighting::Style;

use crate::config::UiTheme;
use crate::diff::{FileDiff, LineAccess};
use crate::view::file::view_model::ConflictRegions;

use super::line::LineRenderer;
use super::separator::render_separator;

/// Scrolloff-aware first-visible row, clamped so the window stays in range.
pub(crate) fn scroll_offset(
    selected: usize,
    current: usize,
    scrolloff: usize,
    height: usize,
    total: usize,
) -> usize {
    if height == 0 {
        return current;
    }
    let mut offset = current;
    if selected < offset + scrolloff {
        offset = selected.saturating_sub(scrolloff);
    } else if selected + scrolloff >= offset + height {
        offset = (selected + scrolloff + 1).saturating_sub(height);
    }
    offset.min(total.saturating_sub(height))
}

/// Immutable inputs needed to lay out file view rows for one frame.
pub(crate) struct RowBuilder<'a> {
    pub diff: &'a FileDiff,
    pub old_lines: LineAccess<'a>,
    pub new_lines: LineAccess<'a>,
    pub syntax_opt: Option<&'a Vec<Vec<(Style, String)>>>,
    pub theme: &'a UiTheme,
    pub hscroll: usize,
    pub area_width: u16,
    pub use_gradient: bool,
    pub context_lines: usize,
    pub is_folded: bool,
    pub default_staged: bool,
    pub selected_row_idx: usize,
    pub regions: &'a ConflictRegions,
    /// Unfolded conflict under the cursor: `(index, side space would select)`.
    /// Only that side's marker (`<<<<<<<` for ours, `>>>>>>>` for theirs)
    /// takes the frame highlight; content lines stay plain.
    pub preview: Option<(usize, crate::diff::Side)>,
}

impl<'a> RowBuilder<'a> {
    /// True when `line` is the hovered side's marker: `<<<<<<<` for ours,
    /// `>>>>>>>` for theirs. Only markers take the frame highlight.
    fn is_preview(&self, line: usize, content: &str) -> bool {
        let Some((ci, side)) = self.preview else {
            return false;
        };
        let Some(range) = self.regions.ranges.get(ci) else {
            return false;
        };
        if !range.contains(&line) {
            return false;
        }
        let ours_marker = content.starts_with("<<<<<<<") && side == crate::diff::Side::Ours;
        let theirs_marker = content.starts_with(">>>>>>>") && side == crate::diff::Side::Theirs;
        ours_marker || theirs_marker
    }

    /// Appends the rows inside `window` and returns the total visual row count.
    ///
    /// `window: None` renders nothing and only counts: the cheap first pass
    /// that lets the caller clamp its scroll offset before the real build.
    pub(crate) fn build(&self, window: Option<Range<usize>>, rows: &mut Vec<Row<'a>>) -> usize {
        let diff = self.diff;
        let theme = self.theme;
        let mut total = 0usize;
        let mut selection_idx = 0usize;
        let mut current_new = 0usize;
        let mut visual_row_idx = 0usize;
        let mut in_conflict = false;

        macro_rules! push_row {
            ($row:expr) => {{
                total += 1;
                if window.as_ref().is_some_and(|w| w.contains(&visual_row_idx)) {
                    rows.push($row);
                }
            }};
        }

        // Conflict whose header was emitted and whose footer is still pending.
        let mut open_frame: Option<usize> = None;
        // Emits the footer closing conflict `ci` as a full-width row.
        macro_rules! push_footer {
            ($ci:expr) => {{
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(conflict_frame_row(
                    self.regions.footer($ci),
                    is_selected,
                    self.area_width,
                    self.use_gradient,
                    theme,
                ));
                visual_row_idx += 1;
            }};
        }

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(self.context_lines);
            if self.is_folded && current_new < context_start {
                let hidden_count = context_start - current_new;
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(render_separator(
                    hidden_count,
                    Some(hunk.before_lines.start),
                    Some(hunk.after_lines.start),
                    is_selected,
                    theme,
                    self.hscroll,
                ));
                current_new = context_start;
                visual_row_idx += 1;
            }

            // Print unchanged lines up to the hunk
            while current_new < hunk_new_start && current_new < self.new_lines.len() {
                // Folded conflicts hide their lines even when the diff leaves
                // them as inter-hunk context (e.g. base lines matching base).
                if self.regions.folded_span(current_new).is_some() {
                    current_new += 1;
                    continue;
                }
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(LineRenderer::builder()
                    .content(self.new_lines.line(current_new))
                    .idx(current_new)
                    .is_selected(is_selected)
                    .is_staged(true)
                    .is_conflict(conflict_flags(
                        &mut in_conflict,
                        self.new_lines.line(current_new)
                    ))
                    .syntax_opt(self.syntax_opt)
                    .area_width(self.area_width)
                    .use_gradient(self.use_gradient)
                    .theme(theme)
                    .hscroll(self.hscroll)
                    .build()
                    .render());
                current_new += 1;
                visual_row_idx += 1;
            }

            // Print all rich lines within the hunk
            let mut is_first_line_of_hunk = true;
            for diff_line in &hunk.lines {
                let is_selected = visual_row_idx == self.selected_row_idx;
                let is_staged = diff.line_selections.get(selection_idx, self.default_staged);
                selection_idx += 1;

                // A folded conflict shows its header; hidden lines (markers,
                // base, losing side) are skipped, kept lines fall through.
                let folded = match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. }
                    | crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                        self.regions.folded_span(*new_line_idx)
                    }
                    crate::diff::DiffLine::Deletion { .. } => self.regions.folded_span(current_new),
                };
                if let Some((ci, is_first)) = folded {
                    if is_first && open_frame != Some(ci) {
                        let is_selected = visual_row_idx == self.selected_row_idx;
                        push_row!(conflict_frame_row(
                            self.regions.header(ci),
                            is_selected,
                            self.area_width,
                            self.use_gradient,
                            theme,
                        ));
                        visual_row_idx += 1;
                        open_frame = Some(ci);
                    }
                    if let crate::diff::DiffLine::Context { new_line_idx, .. }
                    | crate::diff::DiffLine::Addition { new_line_idx, .. } = diff_line
                    {
                        current_new = *new_line_idx + 1;
                    }
                    continue;
                }

                // Past the framed range: close it with a footer row.
                if let Some(ci) = open_frame {
                    let past_end = match diff_line {
                        crate::diff::DiffLine::Context { new_line_idx, .. }
                        | crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                            *new_line_idx >= self.regions.ranges[ci].end
                        }
                        crate::diff::DiffLine::Deletion { .. } => {
                            current_new >= self.regions.ranges[ci].end
                        }
                    };
                    if past_end {
                        push_footer!(ci);
                        open_frame = None;
                    }
                }

                let line_mode = if is_first_line_of_hunk {
                    hunk.marker
                } else {
                    crate::diff::HunkMarker::default()
                };
                is_first_line_of_hunk = false;

                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        let line = self.new_lines.get(*new_line_idx).unwrap_or("");
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*new_line_idx)
                            .is_selected(is_selected)
                            .is_staged(is_staged && !self.regions.covers(*new_line_idx))
                            .is_conflict(conflict_flags(&mut in_conflict, line))
                            .is_preview(self.is_preview(*new_line_idx, line))
                            .mode(line_mode)
                            .syntax_opt(self.syntax_opt)
                            .area_width(self.area_width)
                            .use_gradient(self.use_gradient)
                            .theme(theme)
                            .hscroll(self.hscroll)
                            .build()
                            .render());
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Deletion {
                        old_line_idx,
                        inline_highlights,
                    } => {
                        let line = self.old_lines.get(*old_line_idx).unwrap_or("");
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*old_line_idx)
                            .is_del(true)
                            .is_selected(is_selected)
                            .is_staged(is_staged)
                            .is_conflict(in_conflict)
                            .mode(line_mode)
                            .inline_highlights(inline_highlights)
                            .area_width(self.area_width)
                            .use_gradient(self.use_gradient)
                            .theme(theme)
                            .hscroll(self.hscroll)
                            .build()
                            .render());
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Addition {
                        new_line_idx,
                        inline_highlights,
                    } => {
                        let line = self.new_lines.get(*new_line_idx).unwrap_or("");
                        // Marker lines are context stripped on write, never
                        // hunk content: no +/- sign, no staged tint. Every
                        // other conflict line keeps its sign but never shows
                        // staged colors either: conflict hunks are not
                        // stageable, so identical states always look identical.
                        let is_marker = crate::diff::conflict::is_marker_line(line);
                        let covered = self.regions.covers(*new_line_idx);
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*new_line_idx)
                            .is_add(!is_marker)
                            .is_selected(is_selected)
                            .is_staged(is_staged && !is_marker && !covered)
                            .is_conflict(conflict_flags(&mut in_conflict, line))
                            .is_preview(self.is_preview(*new_line_idx, line))
                            .mode(line_mode)
                            .inline_highlights(inline_highlights)
                            .syntax_opt(self.syntax_opt)
                            .area_width(self.area_width)
                            .use_gradient(self.use_gradient)
                            .theme(theme)
                            .hscroll(self.hscroll)
                            .build()
                            .render());
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                }
            }

            // End of hunk: close the frame when its range was fully scanned.
            if let Some(ci) = open_frame {
                if current_new >= self.regions.ranges[ci].end {
                    push_footer!(ci);
                    open_frame = None;
                }
            }

            if self.is_folded {
                let next_hunk_start = diff
                    .hunks
                    .get(i + 1)
                    .map(|h| h.after_lines.start)
                    .unwrap_or(self.new_lines.len());
                let context_end = current_new
                    .saturating_add(self.context_lines)
                    .min(next_hunk_start);

                while current_new < context_end && current_new < self.new_lines.len() {
                    // Same folded-conflict hiding as the pre-hunk gap above.
                    if self.regions.folded_span(current_new).is_some() {
                        current_new += 1;
                        continue;
                    }
                    let is_selected = visual_row_idx == self.selected_row_idx;
                    push_row!(LineRenderer::builder()
                        .content(self.new_lines.line(current_new))
                        .idx(current_new)
                        .is_selected(is_selected)
                        .is_staged(true)
                        .syntax_opt(self.syntax_opt)
                        .area_width(self.area_width)
                        .use_gradient(self.use_gradient)
                        .theme(theme)
                        .hscroll(self.hscroll)
                        .build()
                        .render());
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        if !self.is_folded {
            while current_new < self.new_lines.len() {
                // Folded frames hide their lines even in the tail: markers
                // and kept lines always live inside a hunk, so anything
                // folded here is hidden base content.
                if self.regions.folded_span(current_new).is_some() {
                    current_new += 1;
                    continue;
                }
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(LineRenderer::builder()
                    .content(self.new_lines.line(current_new))
                    .idx(current_new)
                    .is_selected(is_selected)
                    .is_staged(true)
                    .is_conflict(conflict_flags(
                        &mut in_conflict,
                        self.new_lines.line(current_new)
                    ))
                    .syntax_opt(self.syntax_opt)
                    .area_width(self.area_width)
                    .use_gradient(self.use_gradient)
                    .theme(theme)
                    .hscroll(self.hscroll)
                    .build()
                    .render());
                current_new += 1;
                visual_row_idx += 1;
            }
        } else if current_new < self.new_lines.len() {
            let hidden_count = self.new_lines.len() - current_new;
            let is_selected = visual_row_idx == self.selected_row_idx;
            push_row!(render_separator(
                hidden_count,
                None,
                None,
                is_selected,
                theme,
                self.hscroll,
            ));
        }

        // Trailing flush: a frame ending at the very end of the file.
        if let Some(ci) = open_frame.take() {
            push_footer!(ci);
        }

        total
    }
}

/// Renders a folded conflict's header/footer as a full-width frame row, like
/// fold separators: it takes no line in the file, just the whole width.
/// Orange is the color of conflict: the frame takes a tuned-down orange
/// background with the same gradient logic as staged hunks (gradient by
/// default, solid otherwise).
fn conflict_frame_row<'a>(
    content: &'a str,
    is_selected: bool,
    area_width: u16,
    use_gradient: bool,
    theme: &'a UiTheme,
) -> Row<'a> {
    use super::style::{conflict_orange, conflict_underlay};
    use crate::view::file::utils::colors::safe_lerp_color;
    use ratatui::{
        style::Style,
        text::{Line, Span},
        widgets::{Cell, Row},
    };
    let orange = conflict_orange(theme);
    let accent = conflict_underlay(theme);
    let grad_width = match theme.file_conflict_highlight {
        crate::config::LineHighlightMode::Gradient(pct) if use_gradient => {
            (area_width as f64 * pct).max(1.0) as f32
        }
        _ => 0.0,
    };
    // Same gradient coordinates as staged rows: sign runs 0–1, content from
    // 2, so the wash reads contiguous across the whole row.
    let wash = |x: usize| {
        if grad_width <= 0.0 {
            accent
        } else {
            safe_lerp_color(&accent, &theme.bg, (x as f32 / grad_width).clamp(0.0, 1.0))
        }
    };
    let paint = |bg: crate::config::theme::Color, fg: crate::config::theme::Color| {
        let mut style = Style::default().bg(bg.into()).fg(fg.into());
        if is_selected {
            style = style
                .bg(safe_lerp_color(&theme.cursor_bg, &bg, 0.3).into())
                .fg(theme.fg.into());
        }
        style
    };
    // Per-char spans keep the wash continuous; flat cells would seam.
    let chars_row = |text: &str, start_x: usize| {
        Line::from(
            text.chars()
                .enumerate()
                .map(|(i, c)| Span::styled(c.to_string(), paint(wash(start_x + i), orange)))
                .collect::<Vec<_>>(),
        )
    };

    // Content fades like a staged hunk; padded so the wash spans the row.
    // Delimiter style: short fixed `ours ————————` header, bare
    // `—————————————` rule footer — the wash fills the rest of the row.
    let code_width = (area_width as usize).saturating_sub(8);
    let mut framed = if content.is_empty() {
        "—————————————".to_string()
    } else {
        format!("{content} ————————")
    };
    while framed.chars().count() < code_width {
        framed.push(' ');
    }

    Row::new(vec![
        Cell::from(chars_row(" ", 0)).style(paint(wash(0), orange)),
        // Flat number like staged rows; sign and content fade from x=0/2.
        Cell::from("  ⋮  ").style(paint(accent, orange)),
        Cell::from(chars_row("  ", 0)).style(paint(wash(0), orange)),
        Cell::from(chars_row(&framed, 2)),
    ])
}

/// Tracks whether `line` lies inside a conflict block (markers inclusive).
///
/// A folded conflict is a single line containing both markers; it is treated as
/// a self-contained conflict line without changing the open/closed state.
fn conflict_flags(in_conflict: &mut bool, line: &str) -> bool {
    if line.starts_with("<<<<<<<") {
        if !line.contains(">>>>>>>") {
            *in_conflict = true;
        }
        return true;
    }
    if *in_conflict {
        if line.starts_with(">>>>>>>") {
            *in_conflict = false;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{line_ranges, LineRanges};
    use std::sync::Arc;

    use crate::diff::{DiffLine, Hunk, LineSelections};
    use crate::terminal_colors::TerminalColorMode;
    use crate::theme::ansi_default_theme;

    fn one_hunk_diff() -> FileDiff {
        FileDiff {
            old_file_content: Arc::from("a\nb\nc"),
            new_file_content: Arc::from("a\nB\nc\nd"),
            hunks: vec![Hunk {
                before_lines: 0..3,
                after_lines: 0..4,
                lines: vec![
                    DiffLine::Context {
                        new_line_idx: 0,
                        old_line_idx: 0,
                    },
                    DiffLine::Deletion {
                        old_line_idx: 1,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Addition {
                        new_line_idx: 1,
                        inline_highlights: Vec::new(),
                    },
                    DiffLine::Context {
                        new_line_idx: 2,
                        old_line_idx: 2,
                    },
                ],
                marker: Default::default(),
            }],
            line_selections: LineSelections::default(),
        }
    }

    static EMPTY_REGIONS: ConflictRegions = ConflictRegions {
        ranges: Vec::new(),
        folded: Vec::new(),
        headers: Vec::new(),
        footers: Vec::new(),
        choices: Vec::new(),
        kept: Vec::new(),
        sides: Vec::new(),
    };

    fn builder<'a>(
        diff: &'a FileDiff,
        folded: bool,
        theme: &'a UiTheme,
        ranges: &'a LineRanges,
    ) -> RowBuilder<'a> {
        RowBuilder {
            diff,
            old_lines: LineAccess::new(&diff.old_file_content, ranges),
            new_lines: LineAccess::new(&diff.new_file_content, ranges),
            syntax_opt: None,
            theme,
            hscroll: 0,
            area_width: 80,
            use_gradient: false,
            context_lines: 4,
            is_folded: folded,
            default_staged: true,
            selected_row_idx: 0,
            regions: &EMPTY_REGIONS,
            preview: None,
        }
    }

    /// The window pass must produce exactly the same rows as a full build,
    /// sliced to the window, while the count pass matches both.
    #[test]
    fn window_rows_match_full_build() {
        let diff = one_hunk_diff();
        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let ranges = line_ranges(&diff.new_file_content);

        for folded in [false, true] {
            let b = builder(&diff, folded, &theme, &ranges);
            let mut all = Vec::new();
            let total = b.build(Some(0..usize::MAX), &mut all);
            assert_eq!(total, all.len(), "full build returns the rows it rendered");
            let counted = b.build(None, &mut Vec::new());
            assert_eq!(counted, total, "count pass must equal a full build");
            assert!(total > 3, "fixture must produce several rows");

            let start = 1;
            let end = total.min(3);
            let mut window = Vec::new();
            let total_window = b.build(Some(start..end), &mut window);
            assert_eq!(total_window, total, "window pass reports the same total");
            assert_eq!(window.len(), end - start, "window holds exactly its rows");

            let format = |rows: &[Row]| rows.iter().map(|r| format!("{:?}", r)).collect::<Vec<_>>();
            assert_eq!(
                format(&window),
                format(&all[start..end]),
                "window rows must be byte-identical to the full build slice (folded={folded})"
            );
        }
    }

    #[test]
    fn count_pass_renders_nothing() {
        let diff = one_hunk_diff();
        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let ranges = line_ranges(&diff.new_file_content);
        let b = builder(&diff, false, &theme, &ranges);
        let mut rows = Vec::new();
        assert!(b.build(None, &mut rows) > 0);
        assert!(rows.is_empty());
    }

    #[test]
    fn scroll_offset_follows_selection_and_clamps() {
        // Fresh anchor: clamp to the end of the list.
        assert_eq!(scroll_offset(95, 0, 2, 40, 100), 58);
        // Scrolloff above: pull the window up.
        assert_eq!(scroll_offset(3, 50, 2, 40, 100), 1);
        // Selection inside the window: anchor unchanged.
        assert_eq!(scroll_offset(60, 50, 2, 40, 100), 50);
        // Short list: never scroll past the end.
        assert_eq!(scroll_offset(2, 0, 2, 40, 5), 0);
        // No height: keep the anchor.
        assert_eq!(scroll_offset(2, 7, 2, 0, 100), 7);
    }
}
