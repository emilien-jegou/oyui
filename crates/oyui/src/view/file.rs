use crate::{diff::DiffResult, diff_cache::DiffCache};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use std::collections::HashMap;
use std::path::PathBuf;

use super::{
    ViewAction, CLR_ADD_BG, CLR_ADD_FG, CLR_BG, CLR_CURSOR_BG, CLR_DEL_BG, CLR_DEL_FG, CLR_FG,
};

#[derive(Default)]
pub struct FileViewData {
    pub scroll_states: HashMap<PathBuf, TableState>,
    pub row_counts: HashMap<PathBuf, usize>,
    pub current_path: Option<PathBuf>,
    pub pending_g: bool,
}

impl FileViewData {
    #[tracing::instrument(skip_all)]
    pub fn handle_input(&mut self, key: KeyEvent) -> ViewAction {
        let mut clear_pending = true;
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if let Some(path) = &self.current_path {
            let max_idx = self
                .row_counts
                .get(path)
                .map(|&c| c.saturating_sub(1))
                .unwrap_or(0);
            let state = self.scroll_states.entry(path.clone()).or_default();

            let mut move_cursor = |delta: isize| {
                let current = state.selected().unwrap_or(0) as isize;
                let next = (current + delta).clamp(0, max_idx as isize) as usize;
                state.select(Some(next));
            };

            match (key.code, is_ctrl) {
                (KeyCode::Char('c'), true) => return ViewAction::QuitWithAbort,
                (KeyCode::Char('j'), true) => move_cursor(5),
                (KeyCode::Char('k'), true) => move_cursor(-5),
                (KeyCode::Char('d'), true) => move_cursor(20),
                (KeyCode::Char('u'), true) => move_cursor(-20),

                (KeyCode::Char('q'), false) => return ViewAction::QuitWithAbort,
                (KeyCode::Esc, _) | (KeyCode::Char('h'), _) => return ViewAction::CloseFileView,

                (KeyCode::Char('j'), false) | (KeyCode::Down, _) => move_cursor(1),
                (KeyCode::Char('k'), false) | (KeyCode::Up, _) => move_cursor(-1),

                (KeyCode::Char('G'), false) => state.select(Some(max_idx)),
                (KeyCode::Char('g'), false) => {
                    if self.pending_g {
                        state.select(Some(0));
                        self.pending_g = false;
                        clear_pending = false;
                    } else {
                        self.pending_g = true;
                        clear_pending = false;
                    }
                }
                _ => {}
            }
        } else {
            // No current path, handle globals
            match (key.code, is_ctrl) {
                (KeyCode::Char('c'), true) | (KeyCode::Char('q'), false) => {
                    return ViewAction::QuitWithAbort
                }
                (KeyCode::Esc, _) | (KeyCode::Char('h'), false) => {
                    return ViewAction::CloseFileView
                }
                _ => {}
            }
        }

        if clear_pending {
            self.pending_g = false;
        }

        ViewAction::None
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, cache: &DiffCache) {
        let Some(path) = &self.current_path else {
            return;
        };

        let [header_area, list_area] = Layout::vertical([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // File content
        ])
        .areas(area);

        // -- Draw Header --
        let mut header_spans = vec![Span::styled(
            format!(" {} ", path.display()),
            Style::default().bg(Color::Rgb(40, 40, 50)).fg(CLR_FG),
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
                            Style::default().fg(CLR_ADD_FG),
                        ));
                    }

                    if *deletions != 0 {
                        header_spans.push(Span::styled(
                            format!("-{} ", deletions),
                            Style::default().fg(CLR_DEL_FG),
                        ));
                    }
                }
                crate::diff::DiffStats::Binary { bytes } => {
                    header_spans.push(Span::styled("(binary)", Style::default().fg(Color::Blue)));
                    header_spans.push(Span::styled(
                        format!(" {} bytes", bytes),
                        Style::default().fg(Color::Rgb(40, 40, 50)),
                    ));
                }
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(header_spans)).bg(CLR_BG),
            header_area,
        );

        let Some(diff_result) = cache.diffs.get(path).value() else {
            frame.render_widget(Paragraph::new("Loading...").bg(CLR_BG), list_area);
            return;
        };

        let diff = match diff_result {
            crate::diff::DiffResult::Text(d) => d,
            crate::diff::DiffResult::Empty => {
                frame.render_widget(
                    Paragraph::new("Empty file")
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(Color::DarkGray)),
                    list_area,
                );
                return;
            }
            crate::diff::DiffResult::Binary { size, mime, ext } => {
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
                        .style(Style::default().fg(Color::DarkGray)),
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
                    .style(Style::default().fg(Color::Yellow)),
                    list_area,
                );
                return;
            }
            DiffResult::Error(e) => {
                frame.render_widget(
                    Paragraph::new(format!("Error reading file: {}", e))
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(Color::Red)),
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

        let mut rows = Vec::new();
        let mut current_new = 0;
        let mut visual_row_idx = 0;

        for hunk in &diff.hunks {
            let hunk_new_start = hunk.after_lines.start;

            // Print unchanged lines up to the hunk
            while current_new < hunk_new_start && current_new < new_lines.len() {
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_line(
                    new_lines[current_new],
                    current_new,
                    false,
                    false,
                    is_selected,
                    syntax_opt,
                ));
                current_new += 1;
                visual_row_idx += 1;
            }

            // Print deleted lines (red context)
            for old_idx in hunk.before_lines.clone() {
                if old_idx < old_lines.len() {
                    let is_selected = visual_row_idx == selected_row_idx;
                    rows.push(render_line(
                        old_lines[old_idx],
                        old_idx,
                        false,
                        true,
                        is_selected,
                        None,
                    ));
                    visual_row_idx += 1;
                }
            }

            // Print added lines (green context)
            for new_idx in hunk.after_lines.clone() {
                if new_idx < new_lines.len() {
                    let is_selected = visual_row_idx == selected_row_idx;
                    rows.push(render_line(
                        new_lines[new_idx],
                        new_idx,
                        true,
                        false,
                        is_selected,
                        syntax_opt,
                    ));
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        // Print remaining unchanged lines safely
        while current_new < new_lines.len() {
            let is_selected = visual_row_idx == selected_row_idx;
            rows.push(render_line(
                new_lines[current_new],
                current_new,
                false,
                false,
                is_selected,
                syntax_opt,
            ));
            current_new += 1;
            visual_row_idx += 1;
        }

        // Cache the total mapped row count for `handle_input` limits
        self.row_counts.insert(path.clone(), rows.len());

        let table = Table::new(
            rows,
            [
                Constraint::Length(5), // Left aligned line numbers
                Constraint::Min(0),    // Main code content
            ],
        )
        .block(Block::default().borders(Borders::NONE).bg(CLR_BG));

        frame.render_stateful_widget(table, list_area, scroll_state);
    }
}

fn render_line<'a>(
    content: &'a str,
    idx: usize,
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    syntax_opt: Option<&'a Vec<Vec<(syntect::highlighting::Style, String)>>>,
) -> Row<'a> {
    let row_style = get_line_style(is_add, is_del, is_selected);
    let prefix = if is_add {
        "+ "
    } else if is_del {
        "- "
    } else {
        "  "
    };

    let mut row_spans = vec![Span::styled(prefix, row_style)];

    // Apply syntax highlighting exclusively to un-deleted text if provided
    if !is_del {
        if let Some(syntax_lines) = syntax_opt {
            if let Some(styles) = syntax_lines.get(idx) {
                for (syn_style, text) in styles {
                    let mut base_style = to_tui_style(*syn_style);
                    if is_add {
                        base_style = base_style.fg(CLR_ADD_FG);
                    }
                    row_spans.push(Span::styled(text.clone(), base_style));
                }
            } else {
                row_spans.push(Span::styled(content.to_string(), row_style));
            }
        } else {
            row_spans.push(Span::styled(content.to_string(), row_style));
        }
    } else {
        row_spans.push(Span::styled(content.to_string(), row_style));
    }

    let line_num_style = if is_selected {
        Style::default().bg(Color::Rgb(45, 45, 55)).fg(Color::White)
    } else {
        Style::default()
            .bg(Color::Rgb(20, 20, 25))
            .fg(Color::DarkGray)
    };

    let line_num_span = Span::styled(format!("{:>4} ", idx + 1), line_num_style);

    Row::new(vec![
        Cell::from(line_num_span).style(line_num_style),
        Cell::from(Line::from(row_spans)),
    ])
    .style(row_style)
}

fn get_line_style(is_add: bool, is_del: bool, is_selected: bool) -> Style {
    if is_selected {
        if is_add {
            // Grayish-green
            Style::default().bg(Color::Rgb(45, 65, 45)).fg(CLR_ADD_FG)
        } else if is_del {
            // Grayish-red
            Style::default().bg(Color::Rgb(65, 45, 45)).fg(CLR_DEL_FG)
        } else {
            // Standard gray hover
            Style::default().bg(CLR_CURSOR_BG).fg(CLR_FG)
        }
    } else {
        if is_add {
            Style::default().bg(CLR_ADD_BG).fg(CLR_ADD_FG)
        } else if is_del {
            Style::default().bg(CLR_DEL_BG).fg(CLR_DEL_FG)
        } else {
            Style::default().bg(CLR_BG).fg(CLR_FG)
        }
    }
}

fn to_tui_style(style: syntect::highlighting::Style) -> Style {
    Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ))
}
