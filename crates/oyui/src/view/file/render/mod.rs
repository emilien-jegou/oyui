pub mod line;
pub mod separator;
pub mod style;

use super::FileViewData;
use crate::{
    config::UiTheme,
    diff::{DiffResult, HunkMarker},
    diff_cache::DiffCache,
    tree::FileTree,
};
use line::LineRenderer;
use separator::render_separator;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::Span,
    widgets::Paragraph,
    Frame,
};

impl FileViewData {
    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        cache: &DiffCache,
        tree: &FileTree,
        theme: &UiTheme,
    ) {
        let Some(path) = &self.current_path else {
            return;
        };

        let [header_area, list_area] = Layout::vertical([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // File content
        ])
        .areas(area);

        let mut header_spans = vec![Span::styled(
            format!(" {} ", path.display()),
            Style::default()
                .bg(theme.cursor_bg.into())
                .fg(theme.fg.into()),
        )];

        if let Some(stats) = cache.stats.get(path).value() {
            header_spans.push(Span::raw("  "));
            match stats {
                crate::diff::DiffStats::Text {
                    insertions,
                    deletions,
                } => {
                    if *insertions != 0 {
                        header_spans.push(Span::styled(
                            format!("+{} ", insertions),
                            Style::default().fg(theme.add_fg.into()),
                        ));
                    }

                    if *deletions != 0 {
                        header_spans.push(Span::styled(
                            format!("-{} ", deletions),
                            Style::default().fg(theme.del_fg.into()),
                        ));
                    }
                }
                crate::diff::DiffStats::Binary { bytes } => {
                    header_spans.push(Span::styled(
                        "(binary)",
                        Style::default().fg(theme.dir.into()),
                    ));
                    header_spans.push(Span::styled(
                        format!(" {} bytes", bytes),
                        Style::default().fg(theme.dim.into()),
                    ));
                }
            }
        }

        frame.render_widget(
            Paragraph::new(ratatui::text::Line::from(header_spans)).bg(theme.bg),
            header_area,
        );

        let Some(diff_result) = cache.diffs.get(path).value() else {
            frame.render_widget(Paragraph::new("Loading...").bg(theme.bg), list_area);
            return;
        };

        let diff = match diff_result {
            DiffResult::Text(d) => d,
            DiffResult::Empty => {
                frame.render_widget(
                    Paragraph::new("Empty file")
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(theme.dim.into())),
                    list_area,
                );
                return;
            }
            DiffResult::Binary { size, mime, ext } => {
                let size_str = if *size < 1024 {
                    format!("{} B", size)
                } else if *size < 1024 * 1024 {
                    format!("{:.2} KB", *size as f64 / 1024.0)
                } else {
                    format!("{:.2} MB", *size as f64 / 1024.0 / 1024.0)
                };

                let msg = format!(
                    "Binary file not shown\n(Files differ)\n\nType: {}\nExtension: {}\nSize: {}",
                    mime, ext, size_str
                );

                frame.render_widget(
                    Paragraph::new(msg)
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(theme.dim.into())),
                    list_area,
                );
                return;
            }
            DiffResult::TooLarge(size) => {
                frame.render_widget(
                    Paragraph::new(format!(
                        "File is too large ({} MB) to display inline.",
                        size / 1024 / 1024
                    ))
                    .alignment(ratatui::layout::Alignment::Center)
                    .style(Style::default().fg(theme.partial.into())),
                    list_area,
                );
                return;
            }
            DiffResult::Error(e) => {
                frame.render_widget(
                    Paragraph::new(format!("Error reading file: {}", e))
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(theme.del_fg.into())),
                    list_area,
                );
                return;
            }
        };

        let syntax_opt = cache.syntax.get(path).value();

        let old_lines: Vec<&str> = diff.old_text.lines().collect();
        let new_lines: Vec<&str> = diff.new_text.lines().collect();

        // Get scroll state for hover styling
        let scroll_state = self.scroll_states.entry(path.clone()).or_default();
        let selected_row_idx = scroll_state.selected().unwrap_or(0);

        let hscroll = self.hscroll_states.get(path).copied().unwrap_or(0);
        let area_width = list_area.width;
        let use_gradient = self.use_gradient;

        // Find if this file is selected in the file tree, which sets our default state
        let default_staged = tree
            .get_file_state(path)
            .unwrap_or(crate::tree::StagingState::Unstaged)
            == crate::tree::StagingState::Staged;

        let mut rows = Vec::new();
        let mut line_map = Vec::new();
        let mut hunk_starts_for_file = Vec::new();
        let mut row_to_hunk_for_file = Vec::new();
        let mut selection_idx = 0;
        let mut current_new = 0;
        let mut visual_row_idx = 0;

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(self.context_lines);

            if self.is_folded && current_new < context_start {
                let hidden_count = context_start - current_new;
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_separator(
                    hidden_count,
                    Some(hunk.before_lines.start),
                    Some(hunk.after_lines.start),
                    is_selected,
                    theme,
                    hscroll,
                ));
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
                current_new = context_start;
                visual_row_idx += 1;
            }

            // Print unchanged lines up to the hunk
            while current_new < hunk_new_start && current_new < new_lines.len() {
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(
                    LineRenderer::builder()
                        .content(new_lines[current_new])
                        .idx(current_new)
                        .is_selected(is_selected)
                        .is_staged(true)
                        .syntax_opt(syntax_opt)
                        .area_width(area_width)
                        .use_gradient(use_gradient)
                        .theme(theme)
                        .hscroll(hscroll)
                        .build()
                        .render(),
                );
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }

            // Print all rich lines within the hunk
            let mut recorded_hunk_start = false;
            let mut is_first_line_of_hunk = true;
            for diff_line in &hunk.lines {
                let is_selected = visual_row_idx == selected_row_idx;
                let is_staged = *diff
                    .line_selections
                    .get(selection_idx)
                    .unwrap_or(&default_staged);
                selection_idx += 1;

                let line_mode = if is_first_line_of_hunk {
                    hunk.marker
                } else {
                    HunkMarker::default()
                };
                is_first_line_of_hunk = false;

                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        let line = new_lines.get(*new_line_idx).copied().unwrap_or("");
                        rows.push(
                            LineRenderer::builder()
                                .content(line)
                                .idx(*new_line_idx)
                                .is_selected(is_selected)
                                .is_staged(is_staged)
                                .mode(line_mode)
                                .syntax_opt(syntax_opt)
                                .area_width(area_width)
                                .use_gradient(use_gradient)
                                .theme(theme)
                                .hscroll(hscroll)
                                .build()
                                .render(),
                        );
                        line_map.push(current_new);
                        row_to_hunk_for_file.push(Some(i));
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Deletion {
                        old_line_idx,
                        inline_highlights,
                    } => {
                        if !recorded_hunk_start {
                            hunk_starts_for_file.push(visual_row_idx);
                            recorded_hunk_start = true;
                        }
                        let line = old_lines.get(*old_line_idx).copied().unwrap_or("");
                        rows.push(
                            LineRenderer::builder()
                                .content(line)
                                .idx(*old_line_idx)
                                .is_del(true)
                                .is_selected(is_selected)
                                .is_staged(is_staged)
                                .mode(line_mode)
                                .inline_highlights(inline_highlights)
                                .area_width(area_width)
                                .use_gradient(use_gradient)
                                .theme(theme)
                                .hscroll(hscroll)
                                .build()
                                .render(),
                        );
                        line_map.push(current_new);
                        row_to_hunk_for_file.push(Some(i));
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Addition {
                        new_line_idx,
                        inline_highlights,
                    } => {
                        if !recorded_hunk_start {
                            hunk_starts_for_file.push(visual_row_idx);
                            recorded_hunk_start = true;
                        }
                        let line = new_lines.get(*new_line_idx).copied().unwrap_or("");
                        rows.push(
                            LineRenderer::builder()
                                .content(line)
                                .idx(*new_line_idx)
                                .is_add(true)
                                .is_selected(is_selected)
                                .is_staged(is_staged)
                                .mode(line_mode)
                                .inline_highlights(inline_highlights)
                                .syntax_opt(syntax_opt)
                                .area_width(area_width)
                                .use_gradient(use_gradient)
                                .theme(theme)
                                .hscroll(hscroll)
                                .build()
                                .render(),
                        );
                        line_map.push(current_new);
                        row_to_hunk_for_file.push(Some(i));
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
                    .unwrap_or(new_lines.len());
                let context_end = current_new
                    .saturating_add(self.context_lines)
                    .min(next_hunk_start);

                while current_new < context_end && current_new < new_lines.len() {
                    let is_selected = visual_row_idx == selected_row_idx;
                    rows.push(
                        LineRenderer::builder()
                            .content(new_lines[current_new])
                            .idx(current_new)
                            .is_selected(is_selected)
                            .is_staged(true)
                            .syntax_opt(syntax_opt)
                            .area_width(area_width)
                            .use_gradient(use_gradient)
                            .theme(theme)
                            .hscroll(hscroll)
                            .build()
                            .render(),
                    );
                    line_map.push(current_new);
                    row_to_hunk_for_file.push(None);
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        if !self.is_folded {
            while current_new < new_lines.len() {
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(
                    LineRenderer::builder()
                        .content(new_lines[current_new])
                        .idx(current_new)
                        .is_selected(is_selected)
                        .is_staged(true)
                        .syntax_opt(syntax_opt)
                        .area_width(area_width)
                        .use_gradient(use_gradient)
                        .theme(theme)
                        .hscroll(hscroll)
                        .build()
                        .render(),
                );
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }
        } else {
            if current_new < new_lines.len() {
                let hidden_count = new_lines.len() - current_new;
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_separator(
                    hidden_count,
                    None,
                    None,
                    is_selected,
                    theme,
                    hscroll,
                ));
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
            }
        }

        let total_rows = rows.len();

        self.row_counts.insert(path.clone(), total_rows);
        self.line_mapping.insert(path.clone(), line_map);
        self.hunk_starts.insert(path.clone(), hunk_starts_for_file);
        self.row_to_hunk.insert(path.clone(), row_to_hunk_for_file);
        let table = line::build_line_table(rows, theme);

        let height = list_area.height as usize;
        let width = list_area.width as usize;
        self.last_height = height;
        self.last_width = width;

        if height > 0 {
            let mut offset = scroll_state.offset();
            let scrolloff = self.scrolloff.min(height.saturating_sub(1) / 2);

            if selected_row_idx < offset + scrolloff {
                offset = selected_row_idx.saturating_sub(scrolloff);
            } else if selected_row_idx + scrolloff >= offset + height {
                offset = (selected_row_idx + scrolloff + 1).saturating_sub(height);
            }

            let max_offset = total_rows.saturating_sub(height);
            offset = offset.min(max_offset);

            *scroll_state.offset_mut() = offset;
        }

        frame.render_stateful_widget(table, list_area, scroll_state);
    }
}
