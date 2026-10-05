//! File view rendering: header, hunks, and per-line layout.
pub mod line;
pub mod rows;
pub mod separator;
pub mod style;

use super::FileViewData;
use crate::{config::UiTheme, diff::DiffResult, diff_cache::DiffCache, tree::FileTree};
use rows::{scroll_offset, RowBuilder};

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::Span,
    widgets::{Paragraph, TableState},
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
        let Some(path) = self.current_path.clone() else {
            return;
        };

        let [header_area, list_area] = Layout::vertical([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // File content
        ])
        .areas(area);

        // Find left and right paths to check if names differ
        let mut left_path = None;
        let mut right_path = None;

        if let Some((l, r)) = tree.find_paths(&path) {
            left_path = l;
            right_path = r;
        }

        let left_name = left_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy());
        let right_name = right_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy());

        let name_differ = match (&left_name, &right_name) {
            (Some(l), Some(r)) => l != r,
            _ => false,
        };

        let mut header_spans = Vec::new();

        if name_differ {
            let l_name = left_name.as_ref().unwrap().to_string();
            let r_name = right_name.as_ref().unwrap().to_string();

            header_spans.push(Span::styled(
                " [ ",
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.dim.into()),
            ));
            header_spans.push(Span::styled(
                l_name,
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.fg.into()),
            ));
            header_spans.push(Span::styled(
                " → ",
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.dim.into()),
            ));
            header_spans.push(Span::styled(
                r_name,
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.fg.into()),
            ));
            header_spans.push(Span::styled(
                " ] ",
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.dim.into()),
            ));
        } else {
            header_spans.push(Span::styled(
                format!(" {} ", path.display()),
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.fg.into()),
            ));
        }

        if let Some(stats) = cache.stats.get(&path).as_deref() {
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

        let df = cache.diffs.get(&path);
        let Some(diff_result) = df.as_deref() else {
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

        let syntax_df = cache.syntax.get(&path);
        let syntax_opt = syntax_df.as_deref();

        let old_lines: Vec<&str> = diff.old_file_content.lines().collect();
        let new_lines: Vec<&str> = diff.new_file_content.lines().collect();

        // Recompute the view model if needed
        self.recompute_view_model(diff);
        let path = path.clone();

        // Get scroll state for hover styling
        let scroll_state = self.scroll_states.entry(path.clone()).or_default();
        let selected_row_idx = scroll_state.selected().unwrap_or(0);

        let hscroll = self.hscroll_states.get(&path).copied().unwrap_or(0);
        let area_width = list_area.width;
        let use_gradient = self.use_gradient;

        // Find if this file is selected in the file tree, which sets our default state
        let default_staged = tree
            .get_file_state(&path)
            .unwrap_or(crate::tree::StagingState::Unstaged)
            == crate::tree::StagingState::Staged;

        let builder = RowBuilder {
            diff,
            old_lines: &old_lines,
            new_lines: &new_lines,
            syntax_opt,
            theme,
            hscroll,
            area_width,
            use_gradient,
            context_lines: self.context_lines,
            is_folded: self.is_folded,
            default_staged,
            selected_row_idx,
            regions: &self.conflict_regions,
        };

        // Pass 1 counts every visual row without rendering; pass 2 renders
        // only the visible window, so frames stay O(viewport), not O(diff).
        let mut rows = Vec::new();
        let total_rows = builder.build(None, &mut rows);

        self.last_height = list_area.height as usize;
        self.last_width = list_area.width as usize;
        let height = self.last_height;

        let mut offset = scroll_state.offset();
        if height > 0 {
            let scrolloff = self.scrolloff.min(height.saturating_sub(1) / 2);
            offset = scroll_offset(selected_row_idx, offset, scrolloff, height, total_rows);
            *scroll_state.offset_mut() = offset;
        }

        let start = offset.min(total_rows);
        let end = (start + height).min(total_rows);
        builder.build(Some(start..end), &mut rows);

        let table = line::build_line_table(rows, theme);

        // Slice-local render state: the persisted anchor in `scroll_state`
        // stays absolute while ratatui only ever sees window-relative rows.
        let mut rel_state = TableState::default();
        if end > start {
            rel_state.select(Some(selected_row_idx.clamp(start, end - 1) - start));
        }
        frame.render_stateful_widget(table, list_area, &mut rel_state);
    }
}
