use crate::app::{App, CommandMode};
use crate::config::UiTheme;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

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
    let area = frame.area();
    let [view_area, hint_area, cmd_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let diff_summary = app.get_diff_summary();
    app.view.draw(
        frame,
        view_area,
        &app.tree,
        &app.cache,
        app.base_path.as_ref(),
        diff_summary,
        &app.theme,
    );

    draw_hint_bar(frame, hint_area, &app.command_mode, &app.theme);
    draw_command_bar(frame, cmd_area, &app.command_mode, &app.theme);

    if let CommandMode::ConfirmMerge = app.command_mode {
        let confirm_area = centered_rect(40, 3, frame.area());
        frame.render_widget(Clear, confirm_area);
        frame.render_widget(
            Paragraph::new("Press Enter to Confirm Merge")
                .block(Block::default().borders(Borders::ALL).title(" Merge "))
                .style(Style::default().fg(app.theme.partial)),
            confirm_area,
        );
    }
}

fn draw_hint_bar(frame: &mut Frame, area: Rect, mode: &CommandMode, theme: &UiTheme) {
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
                Span::styled(
                    k,
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}  ", v), Style::default().fg(theme.dim)),
            ]
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg)),
        area,
    );
}

fn draw_command_bar(frame: &mut Frame, area: Rect, mode: &CommandMode, theme: &UiTheme) {
    if let CommandMode::Active(buf) = mode {
        let line = Line::from(vec![
            Span::styled(
                ":",
                Style::default().fg(theme.cmd).add_modifier(Modifier::BOLD),
            ),
            Span::styled(buf, Style::default().fg(theme.fg)),
            Span::styled("▌", Style::default().fg(theme.cmd)),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(theme.cursor_bg)),
            area,
        );
    } else {
        frame.render_widget(
            Paragraph::new("").style(Style::default().bg(theme.bg)),
            area,
        );
    }
}
