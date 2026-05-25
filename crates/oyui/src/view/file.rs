use crate::{
    config::UiTheme,
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

use super::ViewAction;

pub struct FileViewData {
    pub scroll_states: HashMap<PathBuf, TableState>,
    pub row_counts: HashMap<PathBuf, usize>,
    pub line_mapping: HashMap<PathBuf, Vec<usize>>,
    pub hunk_starts: HashMap<PathBuf, Vec<usize>>,
    pub row_to_hunk: HashMap<PathBuf, Vec<Option<usize>>>,
    pub current_path: Option<PathBuf>,
    pub pending_g: bool,
    pub scrolloff: usize,
    pub is_folded: bool,
    pub context_lines: usize,
    pub last_height: usize,
    pub use_gradient: bool,
}

impl Default for FileViewData {
    fn default() -> Self {
        Self {
            scroll_states: HashMap::new(),
            row_counts: HashMap::new(),
            line_mapping: HashMap::new(),
            hunk_starts: HashMap::new(),
            row_to_hunk: HashMap::new(),
            current_path: None,
            pending_g: false,
            scrolloff: 0,
            is_folded: true,
            context_lines: 4,
            last_height: 0,
            use_gradient: true,
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
                (KeyCode::Enter, _) => return ViewAction::ConfirmMerge,
                (KeyCode::Char('c'), true) => return ViewAction::QuitWithAbort,
                (KeyCode::Char('j'), true) => move_cursor(5),
                (KeyCode::Char('k'), true) => move_cursor(-5),
                (KeyCode::Char('d'), true) => move_cursor(20),
                (KeyCode::Char('u'), true) => move_cursor(-20),

                (KeyCode::Char('q'), false) => return ViewAction::QuitWithAbort,
                (KeyCode::Esc, _) | (KeyCode::Char('h'), _) => return ViewAction::CloseFileView,

                (KeyCode::Char('j'), false) | (KeyCode::Down, _) => move_cursor(1),
                (KeyCode::Char('k'), false) | (KeyCode::Up, _) => move_cursor(-1),
                (KeyCode::Char('n'), false) => {
                    if let Some(starts) = self.hunk_starts.get(path) {
                        let target = starts
                            .iter()
                            .find(|&&idx| idx > current_selected)
                            .or_else(|| starts.first());
                        if let Some(&t) = target {
                            next_selected = t;
                            let padding = self.last_height.saturating_sub(1) / 3;
                            next_offset = Some(t.saturating_sub(padding));
                        }
                    }
                }
                (KeyCode::Char('N'), false) => {
                    if let Some(starts) = self.hunk_starts.get(path) {
                        let target = starts
                            .iter()
                            .rev()
                            .find(|&&idx| idx < current_selected)
                            .or_else(|| starts.last());
                        if let Some(&t) = target {
                            next_selected = t;
                            let padding = self.last_height.saturating_sub(1) / 3;
                            next_offset = Some(t.saturating_sub(padding));
                        }
                    }
                }
                (KeyCode::Char(' '), false) => {
                    let mut current_hunk = None;
                    if let Some(mapping) = self.row_to_hunk.get(path) {
                        current_hunk = mapping.get(current_selected).copied().flatten();
                    }
                    if let Some(hunk_idx) = current_hunk {
                        return ViewAction::ToggleStageHunk(hunk_idx);
                    } else {
                        return ViewAction::ToggleStageSelected;
                    }
                }

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
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        cache: &DiffCache,
        tree: &crate::tree::FileTree,
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
            Style::default().bg(theme.cursor_bg).fg(theme.fg),
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
                            Style::default().fg(theme.add_fg),
                        ));
                    }

                    if *deletions != 0 {
                        header_spans.push(Span::styled(
                            format!("-{} ", deletions),
                            Style::default().fg(theme.del_fg),
                        ));
                    }
                }
                crate::diff::DiffStats::Binary { bytes } => {
                    header_spans.push(Span::styled("(binary)", Style::default().fg(theme.dir)));
                    header_spans.push(Span::styled(
                        format!(" {} bytes", bytes),
                        Style::default().fg(theme.dim),
                    ));
                }
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(header_spans)).bg(theme.bg),
            header_area,
        );

        let Some(diff_result) = cache.diffs.get(path).value() else {
            frame.render_widget(Paragraph::new("Loading...").bg(theme.bg), list_area);
            return;
        };

        let diff = match diff_result {
            crate::diff::DiffResult::Text(d) => d,
            crate::diff::DiffResult::Empty => {
                frame.render_widget(
                    Paragraph::new("Empty file")
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(theme.dim)),
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
                        .style(Style::default().fg(theme.dim)),
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
                    .style(Style::default().fg(theme.partial)),
                    list_area,
                );
                return;
            }
            DiffResult::Error(e) => {
                frame.render_widget(
                    Paragraph::new(format!("Error reading file: {}", e))
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().fg(theme.del_fg)),
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
                ));
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
                current_new = context_start;
                visual_row_idx += 1;
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
                    true,
                    &[],
                    syntax_opt,
                    area_width,
                    use_gradient,
                    theme,
                ));
                line_map.push(current_new);
                row_to_hunk_for_file.push(Some(i));
                current_new += 1;
                visual_row_idx += 1;
            }

            // Print all rich lines within the hunk
            let mut recorded_hunk_start = false;
            for diff_line in &hunk.lines {
                let is_selected = visual_row_idx == selected_row_idx;
                let is_staged = *diff
                    .line_selections
                    .get(selection_idx)
                    .unwrap_or(&default_staged);
                selection_idx += 1;

                match diff_line {
                    crate::diff::DiffLine::Context { new_line_idx, .. } => {
                        let line = new_lines.get(*new_line_idx).copied().unwrap_or("");
                        rows.push(render_line(
                            line,
                            *new_line_idx,
                            false,
                            false,
                            is_selected,
                            is_staged,
                            &[],
                            syntax_opt,
                            area_width,
                            use_gradient,
                            theme,
                        ));
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
                        rows.push(render_line(
                            line,
                            *old_line_idx,
                            false,
                            true,
                            is_selected,
                            is_staged,
                            inline_highlights,
                            None,
                            area_width,
                            use_gradient,
                            theme,
                        ));
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
                        rows.push(render_line(
                            line,
                            *new_line_idx,
                            true,
                            false,
                            is_selected,
                            is_staged,
                            inline_highlights,
                            syntax_opt,
                            area_width,
                            use_gradient,
                            theme,
                        ));
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
                    rows.push(render_line(
                        new_lines[current_new],
                        current_new,
                        false,
                        false,
                        is_selected,
                        true,
                        &[],
                        syntax_opt,
                        area_width,
                        use_gradient,
                        theme,
                    ));
                    line_map.push(current_new);
                    row_to_hunk_for_file.push(Some(i));
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
                    true,
                    &[],
                    syntax_opt,
                    area_width,
                    use_gradient,
                    theme,
                ));
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
                current_new += 1;
                visual_row_idx += 1;
            }
        } else {
            if current_new < new_lines.len() {
                let hidden_count = new_lines.len() - current_new;
                let is_selected = visual_row_idx == selected_row_idx;
                rows.push(render_separator(hidden_count, None, None, is_selected, theme));
                line_map.push(current_new);
                row_to_hunk_for_file.push(None);
            }
        }

        let total_rows = rows.len();

        // Cache the total mapped row count for `handle_input` limits
        self.row_counts.insert(path.clone(), total_rows);
        self.line_mapping.insert(path.clone(), line_map);
        self.hunk_starts.insert(path.clone(), hunk_starts_for_file);
        self.row_to_hunk.insert(path.clone(), row_to_hunk_for_file);

        let table = Table::new(
            rows,
            [
                Constraint::Length(1), // Sign column
                Constraint::Length(5), // Left aligned line numbers
                Constraint::Min(0),    // Main code content
            ],
        )
        .column_spacing(0) // Remove gap space to let background styles span beautifully
        .block(Block::default().borders(Borders::NONE).bg(theme.bg));

        let height = list_area.height as usize;
        self.last_height = height;

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

fn lerp_color(c1: Color, c2: Color, t: f32) -> Color {
    let (r1, g1, b1) = match c1 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };
    let (r2, g2, b2) = match c2 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    };
    Color::Rgb(
        (r1 as f32 + (r2 as f32 - r1 as f32) * t).clamp(0.0, 255.0) as u8,
        (g1 as f32 + (g2 as f32 - g1 as f32) * t).clamp(0.0, 255.0) as u8,
        (b1 as f32 + (b2 as f32 - b1 as f32) * t).clamp(0.0, 255.0) as u8,
    )
}

fn render_line<'a>(
    content: &'a str,
    idx: usize,
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    is_staged: bool,
    inline_highlights: &[crate::diff::InlineChange],
    syntax_opt: Option<&'a Vec<Vec<(syntect::highlighting::Style, String)>>>,
    area_width: u16,
    use_gradient: bool,
    theme: &UiTheme,
) -> Row<'a> {
    let row_style = get_line_style(is_add, is_del, is_selected, is_staged, use_gradient, theme);
    let prefix = if is_add {
        "+ "
    } else if is_del {
        "- "
    } else {
        "  "
    };

    let mut row_spans = vec![];
    let mut visual_x = 0;

    let area_width = area_width.max(10);
    // Base gradient spans 50% of the screen
    let grad1_width = (area_width as f32 * 0.5).max(1.0);
    // Layered gradient spans 20% of the screen
    let grad2_width = (area_width as f32 * 0.2).max(1.0);

    let use_grad = use_gradient && (is_add || is_del);

    let end_bg = if is_selected { theme.cursor_bg } else { theme.bg };
    let change_bg = if is_add { theme.add_bg } else { theme.del_bg };
    let accent_bg_grad = lerp_color(theme.bg, theme.partial, 0.4);

    // Helper closure to dynamically generate chars with interpolated background
    let mut push_slice = |slice: &str, style: Style, has_inline: bool| {
        if !use_grad || has_inline {
            row_spans.push(Span::styled(slice.to_string(), style));
            visual_x += slice.chars().count();
            return;
        }

        let mut current_string = String::new();
        let mut current_bg = None;

        for c in slice.chars() {
            let t1 = (visual_x as f32 / grad1_width).clamp(0.0, 1.0);
            let base_bg = lerp_color(change_bg, end_bg, t1);

            let target_bg = if is_staged {
                let t2 = (visual_x as f32 / grad2_width).clamp(0.0, 1.0);
                lerp_color(accent_bg_grad, base_bg, t2)
            } else {
                base_bg
            };

            if Some(target_bg) != current_bg {
                if !current_string.is_empty() {
                    row_spans.push(Span::styled(
                        current_string.clone(),
                        style.bg(current_bg.unwrap()),
                    ));
                    current_string.clear();
                }
                current_bg = Some(target_bg);
            }
            current_string.push(c);
            visual_x += 1;
        }

        if !current_string.is_empty() {
            row_spans.push(Span::styled(
                current_string,
                style.bg(current_bg.unwrap_or(end_bg)),
            ));
        }
    };

    // Push the +/- Prefix
    push_slice(prefix, row_style, false);

    // Establish vivid inline highlight background colors mimicking Difftastic/GitHub
    let inline_bg = if !is_staged {
        if is_selected {
            theme.cursor_bg
        } else {
            theme.bg
        }
    } else if is_selected {
        if is_add {
            lerp_color(theme.add_bg, theme.add_fg, 0.4)
        } else {
            lerp_color(theme.del_bg, theme.del_fg, 0.4)
        }
    } else {
        if is_add {
            lerp_color(theme.add_bg, theme.add_fg, 0.2)
        } else {
            lerp_color(theme.del_bg, theme.del_fg, 0.2)
        }
    };

    let (fg_r, fg_g, fg_b) = match theme.fg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (200, 200, 200),
    };

    let fallback_style = syntect::highlighting::Style {
        foreground: syntect::highlighting::Color {
            r: fg_r,
            g: fg_g,
            b: fg_b,
            a: 255,
        },
        background: syntect::highlighting::Color::WHITE,
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

        // Code and sign column consistently keep their "change" FG color
        if is_add {
            base_style = base_style.fg(theme.add_fg);
        }
        if is_del {
            base_style = base_style.fg(theme.del_fg);
        }

        let mut token_offset = 0;

        while token_offset < text.len() {
            let abs_byte = text_start + token_offset;
            let active_hl = inline_highlights
                .iter()
                .find(|h| h.byte_range.contains(&abs_byte));

            let prev_offset = token_offset;

            if let Some(hl) = active_hl {
                let hl_end_in_token =
                    (hl.byte_range.end.saturating_sub(text_start)).min(text.len());
                if let Some(slice) = text.get(token_offset..hl_end_in_token) {
                    push_slice(slice, base_style.bg(inline_bg), true);
                } else {
                    push_slice(&text[token_offset..], base_style, false);
                    break;
                }
                token_offset = hl_end_in_token;
            } else {
                let next_hl_start = inline_highlights
                    .iter()
                    .map(|h| h.byte_range.start)
                    .filter(|&start| start > abs_byte)
                    .min()
                    .unwrap_or(text_end);

                let next_hl_in_token = (next_hl_start.saturating_sub(text_start)).min(text.len());
                if let Some(slice) = text.get(token_offset..next_hl_in_token) {
                    push_slice(slice, base_style, false);
                } else {
                    push_slice(&text[token_offset..], base_style, false);
                    break;
                }
                token_offset = next_hl_in_token;
            }

            if token_offset <= prev_offset {
                break;
            }
        }
        current_byte = text_end;
    }

    let (sign_char, mut sign_style) = if is_add || is_del {
        let fg = if is_staged {
            theme.partial
        } else {
            theme.dim
        };
        ("▎", Style::default().fg(fg))
    } else {
        (" ", Style::default())
    };

    let line_num_style = if is_selected {
        if is_staged && (is_add || is_del) {
            Style::default()
                .bg(lerp_color(theme.cursor_bg, theme.partial, 0.2))
                .fg(theme.fg)
        } else {
            Style::default().bg(theme.cursor_bg).fg(theme.fg)
        }
    } else if is_staged && (is_add || is_del) {
        Style::default()
            .bg(lerp_color(theme.bg, theme.partial, 0.1))
            .fg(theme.partial)
    } else {
        Style::default().bg(theme.bg).fg(theme.dim)
    };

    sign_style = sign_style.bg(line_num_style.bg.unwrap_or(Color::Reset));

    let sign_span = Span::styled(sign_char, sign_style);
    let line_num_span = Span::styled(format!("{:>4} ", idx + 1), line_num_style);

    Row::new(vec![
        Cell::from(sign_span).style(sign_style),
        Cell::from(line_num_span).style(line_num_style),
        Cell::from(Line::from(row_spans)),
    ])
    .style(row_style)
}

fn get_line_style(
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    is_staged: bool,
    use_gradient: bool,
    theme: &UiTheme,
) -> Style {
    if use_gradient && (is_add || is_del) {
        let bg = if is_selected { theme.cursor_bg } else { theme.bg };
        let mut style = Style::default().bg(bg);
        if is_add {
            style = style.fg(theme.add_fg);
        } else {
            style = style.fg(theme.del_fg);
        }
        return style;
    }

    // Fallback logic when gradient is off
    let mut style = if is_selected {
        if is_add {
            Style::default()
                .bg(lerp_color(theme.cursor_bg, theme.add_bg, 0.6))
                .fg(theme.add_fg)
        } else if is_del {
            Style::default()
                .bg(lerp_color(theme.cursor_bg, theme.del_bg, 0.6))
                .fg(theme.del_fg)
        } else {
            Style::default().bg(theme.cursor_bg).fg(theme.fg)
        }
    } else {
        if is_add {
            Style::default().bg(theme.add_bg).fg(theme.add_fg)
        } else if is_del {
            Style::default().bg(theme.del_bg).fg(theme.del_fg)
        } else {
            Style::default().bg(theme.bg).fg(theme.fg)
        }
    };

    if !is_staged && (is_add || is_del) {
        style = style.fg(theme.dim);
        if !is_selected {
            style = style.bg(theme.bg);
        }
    }

    style
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
    theme: &UiTheme,
) -> Row<'a> {
    let mut style = Style::default()
        .bg(lerp_color(theme.bg, theme.dir, 0.1))
        .fg(lerp_color(theme.dim, theme.dir, 0.5));
    if is_selected {
        style = style.bg(theme.cursor_bg).fg(theme.fg);
    }

    let mut spans = vec![];
    if let (Some(old), Some(new)) = (next_old, next_new) {
        spans.push(Span::styled(
            format!(" @@ -{} +{} @@ ", old + 1, new + 1),
            style.fg(if is_selected {
                theme.fg
            } else {
                lerp_color(theme.dim, theme.dir, 0.8)
            }),
        ));
    }
    spans.push(Span::styled(
        format!(" ⋯ {} hidden lines ⋯ ", hidden_count),
        style,
    ));

    Row::new(vec![
        Cell::from(" ").style(style),
        Cell::from("  ⋮  ").style(style.fg(if is_selected { theme.fg } else { theme.dim })),
        Cell::from(Line::from(spans)).style(style),
    ])
    .style(style)
}
