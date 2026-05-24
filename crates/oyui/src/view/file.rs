use crate::{
    diff::{DiffResult, FileDiff},
    diff_cache::DiffCache,
};
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

pub struct FileViewData {
    pub scroll_states: HashMap<PathBuf, TableState>,
    pub row_counts: HashMap<PathBuf, usize>,
    pub line_mapping: HashMap<PathBuf, Vec<usize>>,
    pub current_path: Option<PathBuf>,
    pub pending_g: bool,
    pub scrolloff: usize,
    pub is_folded: bool,
    pub context_lines: usize,
}

impl Default for FileViewData {
    fn default() -> Self {
        Self {
            scroll_states: HashMap::new(),
            row_counts: HashMap::new(),
            line_mapping: HashMap::new(),
            current_path: None,
            pending_g: false,
            scrolloff: 0,
            is_folded: true,
            context_lines: 4,
        }
    }
}

impl FileViewData {
    fn get_line_map(&self, diff: &FileDiff, new_lines_len: usize) -> Vec<usize> {
        let mut line_map = Vec::new();
        let mut current_new = 0;

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(self.context_lines);

            if self.is_folded && current_new < context_start {
                line_map.push(current_new);
                current_new = context_start;
            }

            while current_new < hunk_new_start && current_new < new_lines_len {
                line_map.push(current_new);
                current_new += 1;
            }

            for diff_line in &hunk.lines {
                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        line_map.push(current_new);
                        current_new = *new_line_idx + 1;
                    }
                    crate::diff::DiffLine::Deletion { .. } => {
                        line_map.push(current_new);
                    }
                    crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                        line_map.push(current_new);
                        current_new = *new_line_idx + 1;
                    }
                }
            }

            if self.is_folded {
                let next_hunk_start = diff
                    .hunks
                    .get(i + 1)
                    .map(|h| h.after_lines.start)
                    .unwrap_or(new_lines_len);
                let context_end = current_new
                    .saturating_add(self.context_lines)
                    .min(next_hunk_start);

                while current_new < context_end && current_new < new_lines_len {
                    line_map.push(current_new);
                    current_new += 1;
                }
            }
        }

        if !self.is_folded {
            while current_new < new_lines_len {
                line_map.push(current_new);
                current_new += 1;
            }
        } else if current_new < new_lines_len {
            line_map.push(current_new);
        }

        line_map
    }

    #[tracing::instrument(skip_all)]
    pub fn handle_input(&mut self, key: KeyEvent, cache: &DiffCache) -> ViewAction {
        let mut clear_pending = true;
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if let Some(path) = &self.current_path {
            let max_idx = self
                .row_counts
                .get(path)
                .map(|&c| c.saturating_sub(1))
                .unwrap_or(0);

            let (current_selected, current_offset) = {
                let s = self.scroll_states.get(path);
                (
                    s.and_then(|st| st.selected()).unwrap_or(0),
                    s.map(|st| st.offset()).unwrap_or(0),
                )
            };
            let screen_y = current_selected.saturating_sub(current_offset);

            let mut next_selected = current_selected;
            let mut next_offset = None;

            let mut move_cursor = |delta: isize| {
                next_selected =
                    (current_selected as isize + delta).clamp(0, max_idx as isize) as usize;
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

                (KeyCode::Char('G'), false) => next_selected = max_idx,
                (KeyCode::Char('f'), false) | (KeyCode::Char('z'), false) => {
                    let mut target_logical = 0;
                    if let Some(mapping) = self.line_mapping.get(path) {
                        target_logical = mapping.get(current_selected).copied().unwrap_or(0);
                    }

                    self.is_folded = !self.is_folded;

                    if let Some(diff_result) = cache.diffs.get(path).value() {
                        if let crate::diff::DiffResult::Text(diff) = diff_result {
                            let new_lines_len = diff.new_text.lines().count();
                            let new_map = self.get_line_map(diff, new_lines_len);

                            next_selected = new_map
                                .iter()
                                .position(|&l| l >= target_logical)
                                .unwrap_or(new_map.len().saturating_sub(1));
                        } else {
                            next_selected = 0;
                        }
                    } else {
                        next_selected = 0;
                    }

                    next_offset = Some(next_selected.saturating_sub(screen_y));
                }
                (KeyCode::Char('g'), false) => {
                    if self.pending_g {
                        next_selected = 0;
                        self.pending_g = false;
                        clear_pending = false;
                    } else {
                        self.pending_g = true;
                        clear_pending = false;
                    }
                }
                _ => {}
            }

            let state = self.scroll_states.entry(path.clone()).or_default();
            state.select(Some(next_selected));
            if let Some(off) = next_offset {
                *state.offset_mut() = off;
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
        let mut line_map = Vec::new();
        let mut current_new = 0;
        let mut visual_row_idx = 0;

        for (i, hunk) in diff.hunks.iter().enumerate() {
            let hunk_new_start = hunk.after_lines.start;
            let context_start = hunk_new_start.saturating_sub(self.context_lines);

            if self.is_folded {
                if current_new < context_start {
                    let hidden_count = context_start - current_new;
                    let is_selected = visual_row_idx == selected_row_idx;
                    rows.push(render_separator(
                        hidden_count,
                        Some(hunk.before_lines.start),
                        Some(hunk.after_lines.start),
                        is_selected,
                    ));
                    line_map.push(current_new);
                    current_new = context_start;
                    visual_row_idx += 1;
                }
            }

            // Print unchanged lines up to the hunk
            while current_new < hunk_new_start && current_new < new_lines.len() {
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_line(
                    new_lines[current_new],
                    current_new,
                    false,
                    false,
                    is_selected,
                    &[],
                    syntax_opt,
                ));
                line_map.push(current_new);
                current_new += 1;
                visual_row_idx += 1;
            }

            // Print all rich lines within the hunk
            for diff_line in &hunk.lines {
                let is_selected = visual_row_idx == selected_row_idx;
                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        let line = new_lines.get(*new_line_idx).copied().unwrap_or("");
                        rows.push(render_line(
                            line,
                            *new_line_idx,
                            false,
                            false,
                            is_selected,
                            &[],
                            syntax_opt,
                        ));
                        line_map.push(current_new);
                        current_new = *new_line_idx + 1;
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Deletion {
                        old_line_idx,
                        inline_highlights,
                    } => {
                        let line = old_lines.get(*old_line_idx).copied().unwrap_or("");
                        rows.push(render_line(
                            line,
                            *old_line_idx,
                            false,
                            true,
                            is_selected,
                            inline_highlights,
                            None,
                        ));
                        line_map.push(current_new);
                        visual_row_idx += 1;
                    }
                    crate::diff::DiffLine::Addition {
                        new_line_idx,
                        inline_highlights,
                    } => {
                        let line = new_lines.get(*new_line_idx).copied().unwrap_or("");
                        rows.push(render_line(
                            line,
                            *new_line_idx,
                            true,
                            false,
                            is_selected,
                            inline_highlights,
                            syntax_opt,
                        ));
                        line_map.push(current_new);
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
                    rows.push(render_line(
                        new_lines[current_new],
                        current_new,
                        false,
                        false,
                        is_selected,
                        &[],
                        syntax_opt,
                    ));
                    line_map.push(current_new);
                    current_new += 1;
                    visual_row_idx += 1;
                }
            }
        }

        // Print remaining unchanged lines safely
        if !self.is_folded {
            while current_new < new_lines.len() {
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_line(
                    new_lines[current_new],
                    current_new,
                    false,
                    false,
                    is_selected,
                    &[],
                    syntax_opt,
                ));
                line_map.push(current_new);
                current_new += 1;
                visual_row_idx += 1;
            }
        } else {
            if current_new < new_lines.len() {
                let hidden_count = new_lines.len() - current_new;
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_separator(hidden_count, None, None, is_selected));
                line_map.push(current_new);
                visual_row_idx += 1;
            }
        }

        let total_rows = rows.len();

        // Cache the total mapped row count for `handle_input` limits
        self.row_counts.insert(path.clone(), total_rows);
        self.line_mapping.insert(path.clone(), line_map);

        let table = Table::new(
            rows,
            [
                Constraint::Length(5), // Left aligned line numbers
                Constraint::Min(0),    // Main code content
            ],
        )
        .block(Block::default().borders(Borders::NONE).bg(CLR_BG));

        let height = list_area.height as usize;
        if height > 0 {
            let mut offset = scroll_state.offset();
            // Prevent scrolloff from overlapping itself if the screen is tiny
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

fn render_line<'a>(
    content: &'a str,
    idx: usize,
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    inline_highlights: &[crate::diff::InlineChange],
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

    // Establish vivid inline highlight background colors mimicking Difftastic/GitHub
    let inline_bg = if is_selected {
        if is_add {
            Color::Rgb(65, 130, 65)
        } else {
            Color::Rgb(150, 65, 65)
        }
    } else {
        if is_add {
            Color::Rgb(40, 100, 40)
        } else {
            Color::Rgb(120, 40, 40)
        }
    };

    // Construct a fallback syntax token for lines lacking syntect data (or deleted lines)
    let fallback_style = syntect::highlighting::Style {
        foreground: syntect::highlighting::Color {
            r: 200,
            g: 200,
            b: 200,
            a: 255,
        },
        background: syntect::highlighting::Color::WHITE, // ignored
        font_style: syntect::highlighting::FontStyle::empty(),
    };
    let fallback_tokens = vec![(fallback_style, content.to_string())];

    let tokens = if !is_del {
        syntax_opt
            .and_then(|lines| lines.get(idx))
            .filter(|t| !t.is_empty())
            .unwrap_or(&fallback_tokens)
    } else {
        &fallback_tokens
    };

    let mut current_byte = 0;

    for (syn_style, text) in tokens {
        let text_start = current_byte;
        let text_end = text_start + text.len();

        let mut base_style = to_tui_style(*syn_style);
        if is_add {
            base_style = base_style.fg(CLR_ADD_FG);
        }
        if is_del {
            base_style = base_style.fg(CLR_DEL_FG);
        }

        let mut token_offset = 0;

        while token_offset < text.len() {
            let abs_byte = text_start + token_offset;
            let active_hl = inline_highlights
                .iter()
                .find(|h| h.byte_range.contains(&abs_byte));

            let prev_offset = token_offset;

            if let Some(hl) = active_hl {
                // Slice up to the end of the highlight, or the end of the text chunk (whichever comes first)
                let hl_end_in_token =
                    (hl.byte_range.end.saturating_sub(text_start)).min(text.len());

                // .get() safely handles misaligned unicode byte bounds without panicking
                if let Some(slice) = text.get(token_offset..hl_end_in_token) {
                    row_spans.push(Span::styled(slice.to_string(), base_style.bg(inline_bg)));
                } else {
                    row_spans.push(Span::styled(text[token_offset..].to_string(), base_style));
                    break;
                }
                token_offset = hl_end_in_token;
            } else {
                // Find where the NEXT highlight begins within this token
                let next_hl_start = inline_highlights
                    .iter()
                    .map(|h| h.byte_range.start)
                    .filter(|&start| start > abs_byte)
                    .min()
                    .unwrap_or(text_end);

                let next_hl_in_token = (next_hl_start.saturating_sub(text_start)).min(text.len());

                if let Some(slice) = text.get(token_offset..next_hl_in_token) {
                    row_spans.push(Span::styled(slice.to_string(), base_style));
                } else {
                    row_spans.push(Span::styled(text[token_offset..].to_string(), base_style));
                    break;
                }
                token_offset = next_hl_in_token;
            }

            // Fallback lock prevention: Ensure we continually step forward
            if token_offset <= prev_offset {
                break;
            }
        }
        current_byte = text_end;
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

fn render_separator<'a>(
    hidden_count: usize,
    next_old: Option<usize>,
    next_new: Option<usize>,
    is_selected: bool,
) -> Row<'a> {
    let mut style = Style::default()
        .bg(Color::Rgb(8, 8, 11))
        .fg(Color::Rgb(100, 130, 180));
    if is_selected {
        style = style.bg(CLR_CURSOR_BG).fg(Color::White);
    }

    let mut spans = vec![];
    if let (Some(old), Some(new)) = (next_old, next_new) {
        spans.push(Span::styled(
            format!(" @@ -{} +{} @@ ", old + 1, new + 1),
            style.fg(if is_selected {
                Color::White
            } else {
                Color::Rgb(130, 160, 220)
            }),
        ));
    }
    spans.push(Span::styled(
        format!(" ⋯ {} hidden lines ⋯ ", hidden_count),
        style,
    ));

    Row::new(vec![
        Cell::from("  ⋮  ").style(style.fg(if is_selected {
            Color::White
        } else {
            Color::DarkGray
        })),
        Cell::from(Line::from(spans)).style(style),
    ])
    .style(style)
}
