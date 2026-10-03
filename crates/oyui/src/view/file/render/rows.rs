//! File view row building: counts all visual rows, renders only a window.

use std::ops::Range;

use ratatui::widgets::Row;
use syntect::highlighting::Style;

use crate::config::UiTheme;
use crate::diff::FileDiff;

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
    pub old_lines: &'a [&'a str],
    pub new_lines: &'a [&'a str],
    pub syntax_opt: Option<&'a Vec<Vec<(Style, String)>>>,
    pub theme: &'a UiTheme,
    pub hscroll: usize,
    pub area_width: u16,
    pub use_gradient: bool,
    pub context_lines: usize,
    pub is_folded: bool,
    pub default_staged: bool,
    pub selected_row_idx: usize,
}

impl<'a> RowBuilder<'a> {
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

        macro_rules! push_row {
            ($row:expr) => {{
                total += 1;
                if window.as_ref().is_some_and(|w| w.contains(&visual_row_idx)) {
                    rows.push($row);
                }
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
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(LineRenderer::builder()
                    .content(self.new_lines[current_new])
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

            // Print all rich lines within the hunk
            let mut is_first_line_of_hunk = true;
            for diff_line in &hunk.lines {
                let is_selected = visual_row_idx == self.selected_row_idx;
                let is_staged = diff.line_selections.get(selection_idx, self.default_staged);
                selection_idx += 1;

                let line_mode = if is_first_line_of_hunk {
                    hunk.marker
                } else {
                    crate::diff::HunkMarker::default()
                };
                is_first_line_of_hunk = false;

                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        let line = self.new_lines.get(*new_line_idx).copied().unwrap_or("");
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*new_line_idx)
                            .is_selected(is_selected)
                            .is_staged(is_staged)
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
                        let line = self.old_lines.get(*old_line_idx).copied().unwrap_or("");
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*old_line_idx)
                            .is_del(true)
                            .is_selected(is_selected)
                            .is_staged(is_staged)
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
                        let line = self.new_lines.get(*new_line_idx).copied().unwrap_or("");
                        push_row!(LineRenderer::builder()
                            .content(line)
                            .idx(*new_line_idx)
                            .is_add(true)
                            .is_selected(is_selected)
                            .is_staged(is_staged)
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
                    let is_selected = visual_row_idx == self.selected_row_idx;
                    push_row!(LineRenderer::builder()
                        .content(self.new_lines[current_new])
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
                let is_selected = visual_row_idx == self.selected_row_idx;
                push_row!(LineRenderer::builder()
                    .content(self.new_lines[current_new])
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

        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn builder<'a>(
        diff: &'a FileDiff,
        lines: &'a [&'a str],
        folded: bool,
        theme: &'a UiTheme,
    ) -> RowBuilder<'a> {
        RowBuilder {
            diff,
            old_lines: lines,
            new_lines: lines,
            syntax_opt: None,
            theme,
            hscroll: 0,
            area_width: 80,
            use_gradient: false,
            context_lines: 4,
            is_folded: folded,
            default_staged: true,
            selected_row_idx: 0,
        }
    }

    /// The window pass must produce exactly the same rows as a full build,
    /// sliced to the window, while the count pass matches both.
    #[test]
    fn window_rows_match_full_build() {
        let diff = one_hunk_diff();
        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();

        for folded in [false, true] {
            let b = builder(&diff, &new_lines, folded, &theme);
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
        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();
        let b = builder(&diff, &new_lines, false, &theme);
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
