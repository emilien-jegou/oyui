use crate::app::{App, CommandMode, ViewMode};
use crate::view::TreeRow;
use core_lib::tree::StagingState;
use ratatui::style::Stylize;
use ratatui::widgets::{Borders, Clear};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

const CLR_BG: Color = Color::Rgb(14, 14, 18);
const CLR_CURSOR_BG: Color = Color::Rgb(30, 30, 42);
const CLR_FG: Color = Color::Rgb(200, 200, 210);
const CLR_DIM: Color = Color::Rgb(70, 70, 85);
const CLR_STAGED: Color = Color::Rgb(130, 210, 150);
const CLR_UNSTAGED: Color = Color::Rgb(210, 100, 100);
const CLR_PARTIAL: Color = Color::Rgb(210, 170, 80);
const CLR_DIR: Color = Color::Rgb(110, 150, 220);
const CLR_CMD: Color = Color::Rgb(180, 140, 255);

const CLR_ADD_BG: Color = Color::Rgb(30, 45, 30); // Green tint
const CLR_DEL_BG: Color = Color::Rgb(45, 30, 30); // Red tint
const CLR_ADD_FG: Color = Color::Rgb(150, 255, 150);
const CLR_DEL_FG: Color = Color::Rgb(255, 150, 150);

fn get_line_style(is_add: bool, is_del: bool) -> Style {
    if is_add {
        Style::default().bg(CLR_ADD_BG).fg(CLR_ADD_FG)
    } else if is_del {
        Style::default().bg(CLR_DEL_BG).fg(CLR_DEL_FG)
    } else {
        Style::default().fg(CLR_FG)
    }
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Layout::vertical([Constraint::Length(height)])
        .flex(ratatui::layout::Flex::Center)
        .split(
            Layout::horizontal([Constraint::Length(width)])
                .flex(ratatui::layout::Flex::Center)
                .split(r)[0],
        )[0]
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    match &app.view_mode {
        ViewMode::Tree => draw_tree_view(frame, app),
        ViewMode::FileView(_) => draw_file_view(frame, app),
    }

    if let CommandMode::ConfirmMerge = app.command_mode {
        let area = centered_rect(40, 3, frame.area());
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new("Press Enter to Confirm Merge")
                .block(Block::default().borders(Borders::ALL).title(" Merge "))
                .style(Style::default().fg(Color::Yellow)),
            area,
        );
    }
}

fn draw_tree_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let [body, hint_area, cmd_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_tree_body(frame, app, body);
    draw_hint_bar(frame, hint_area, &app.command_mode);
    draw_command_bar(frame, cmd_area, &app.command_mode);
}

fn draw_tree_body(frame: &mut Frame, app: &App, area: Rect) {
    let rows = app.flat_rows();
    let items: Vec<ListItem> = rows.iter().map(render_tree_row).collect();

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected_index));

    let list = List::new(items)
        .block(Block::default().style(Style::default().bg(CLR_BG)))
        .highlight_style(Style::default().bg(CLR_CURSOR_BG));

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_tree_row(row: &TreeRow) -> ListItem<'_> {
    let mut spans = Vec::new();

    // 1. Draw connecting lines
    for &has_sibling in &row.parent_continuations {
        if has_sibling {
            spans.push(Span::styled("│  ", Style::default().fg(CLR_DIM)));
        } else {
            spans.push(Span::raw("   "));
        }
    }

    let connector = if row.is_last {
        "└── "
    } else {
        "├── "
    };
    spans.push(Span::styled(connector, Style::default().fg(CLR_DIM)));

    // 2. Staging Indicator
    let (stage_sym, stage_color) = match row.staging_state {
        StagingState::Staged => ("●", CLR_STAGED),
        StagingState::Unstaged => ("○", CLR_UNSTAGED),
        StagingState::PartiallyStaged => ("◐", CLR_PARTIAL),
    };
    spans.push(Span::styled(stage_sym, Style::default().fg(stage_color)));
    spans.push(Span::raw(" "));

    // 3. Icons and Name
    if row.is_dir {
        let arrow = if row.is_folded { "▸ " } else { "▾ " };
        spans.push(Span::styled(arrow, Style::default().fg(CLR_FG)));
        spans.push(Span::styled(" ", Style::default().fg(CLR_DIR)));
        spans.push(Span::styled(
            row.name.as_str(),
            Style::default().fg(CLR_DIR).add_modifier(Modifier::BOLD),
        ));
    } else {
        let icon = get_file_icon(&row.name);
        spans.push(Span::styled(icon.0, Style::default().fg(icon.1)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(row.name.as_str(), Style::default().fg(CLR_FG)));
    }

    // 4. Stats
    if let Some(s) = &row.stats {
        spans.push(Span::styled(
            format!("  +{} -{}", s.insertions, s.deletions),
            Style::default().fg(CLR_DIM),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn get_file_icon(name: &str) -> (&'static str, Color) {
    let ext = name.split('.').last().unwrap_or("");
    match ext {
        "tsx" | "jsx" => ("", Color::Rgb(80, 200, 255)),
        "ts" | "js" => ("", Color::Rgb(240, 220, 80)),
        "svg" => ("󰕙", Color::Rgb(255, 180, 100)),
        "md" => ("", Color::Rgb(200, 200, 200)),
        "json" => ("", Color::Rgb(250, 200, 50)),
        _ => ("󰈚", Color::Rgb(180, 180, 190)),
    }
}

fn draw_hint_bar(frame: &mut Frame, area: Rect, mode: &CommandMode) {
    let hints = match mode {
        CommandMode::Normal => vec![
            ("j/k", "move"),
            ("h/l", "close dir / open file"),
            ("space", "stage"),
            ("enter", "open"),
            (":", "cmd"),
            ("q", "quit"),
        ],
        CommandMode::Active(_) => vec![("enter", "run"), ("esc", "cancel")],
        CommandMode::ConfirmMerge => vec![("enter", "run"), ("esc", "cancel")],
    };
    let spans: Vec<Span> = hints
        .into_iter()
        .flat_map(|(k, v)| {
            vec![
                Span::styled(k, Style::default().fg(CLR_FG).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" {}  ", v), Style::default().fg(CLR_DIM)),
            ]
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(CLR_BG)),
        area,
    );
}

fn draw_command_bar(frame: &mut Frame, area: Rect, mode: &CommandMode) {
    if let CommandMode::Active(buf) = mode {
        let line = Line::from(vec![
            Span::styled(
                ":",
                Style::default().fg(CLR_CMD).add_modifier(Modifier::BOLD),
            ),
            Span::styled(buf, Style::default().fg(CLR_FG)),
            Span::styled("▌", Style::default().fg(CLR_CMD)),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Rgb(20, 20, 30))),
            area,
        );
    }
}

fn draw_file_view(frame: &mut Frame, app: &mut App) {
    let ViewMode::FileView(path) = &app.view_mode else {
        return;
    };
    let Some(diff) = app.cache.get_diff(path).value() else {
        frame.render_widget(Paragraph::new("Loading..."), frame.area());
        return;
    };

    let mut lines = Vec::new();
    let total_lines = diff.new_text.lines().count(); // Simplified for now

    for i in 0..total_lines {
        // Check if this line is in any hunk
        let hunk = diff.hunks.iter().find(|h| h.after_lines.contains(&i));

        let is_add = hunk.is_some();
        let content = diff.new_text.lines().nth(i).unwrap_or("");

        let mut row_spans = Vec::new();

        // Add +/- prefix
        let prefix = if is_add { "+ " } else { "  " };
        row_spans.push(Span::styled(prefix, get_line_style(is_add, false)));

        // Syntax highlighting tokens
        if let Some(styles) = diff.highlighted_new.get(i) {
            for (style, text) in styles {
                // We blend the syntax FG with our diff FG if it's a change
                let mut base_style = to_tui_style(*style);
                if is_add {
                    base_style = base_style.fg(CLR_ADD_FG);
                }
                row_spans.push(Span::styled(text.clone(), base_style));
            }
        } else {
            row_spans.push(Span::styled(
                content.to_string(),
                get_line_style(is_add, false),
            ));
        }

        lines.push(ListItem::new(Line::from(row_spans)).style(get_line_style(is_add, false)));
    }

    // Scrollable list
    let list = List::new(lines)
        .block(Block::default().borders(Borders::NONE).bg(CLR_BG))
        .highlight_style(Style::default().bg(CLR_CURSOR_BG));

    // You'd need a list_state in your App to handle j/k scrolling
    frame.render_stateful_widget(list, frame.area(), &mut app.file_scroll_state);
}

/// Helper to convert Syntect style to Ratatui style
fn to_tui_style(style: syntect::highlighting::Style) -> Style {
    Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ))
}
