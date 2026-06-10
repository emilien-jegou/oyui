use crate::app::{App, CommandMode};
use crate::config::UiTheme;
use crate::view::ViewKind;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let [view_area, hint_area, cmd_area] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    if let Some(theme) = app.theme.value() {
        if let Some(ref err) = *app.config_error.read() {
            crate::view::config_error::draw(frame, view_area, err, theme);
        } else if let Some(theme) = app.theme.value() {
            let diff_summary = app.get_diff_summary();

            let tree_guard = app.tree.read();
            let cache_guard = app.cache.read();

            app.view.draw(
                frame,
                view_area,
                &tree_guard,
                &cache_guard,
                app.base_path.as_ref(),
                diff_summary,
                theme,
            );
        }
        draw_hint_bar(
            frame,
            hint_area,
            &app.command_mode,
            &app.view.current.read(),
            theme,
        );
        draw_command_bar(frame, cmd_area, &app.command_mode, theme);

        if let CommandMode::ConfirmMerge = app.command_mode {
            let merge_stats = app.get_merge_stats();
            crate::view::confirm_window::draw(frame, theme, merge_stats);
        }
    }
}

fn draw_hint_bar(
    frame: &mut Frame,
    area: Rect,
    mode: &CommandMode,
    view: &ViewKind,
    theme: &UiTheme,
) {
    let hints = match mode {
        CommandMode::Normal => match view {
            ViewKind::Tree => vec![
                ("j/k", "move"),
                ("h/l", "close/open"),
                ("space", "stage"),
                ("i", "invert"),
                (":", "cmd"),
                ("enter", "merge"),
                ("q", "quit"),
            ],
            ViewKind::File => vec![
                ("j/k", "move"),
                ("n/N", "hunks"),
                ("space", "stage"),
                ("z", "unfold"),
                ("s", "split"),
                ("t", "line"),
                ("h/esc", "back"),
                ("enter", "merge"),
                ("q", "quit"),
            ],
        },
        CommandMode::Active(_) => vec![("enter", "run"), ("esc", "cancel")],
        CommandMode::ConfirmMerge => vec![("enter", "confirm"), ("q/esc", "cancel")],
    };

    let spans: Vec<Span> = hints
        .into_iter()
        .flat_map(|(k, v)| {
            vec![
                Span::styled(
                    k,
                    Style::default()
                        .fg(theme.fg.into())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}  ", v), Style::default().fg(theme.dim.into())),
            ]
        })
        .collect();

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.bg.into())),
        area,
    );
}

fn draw_command_bar(frame: &mut Frame, area: Rect, mode: &CommandMode, theme: &UiTheme) {
    if let CommandMode::Active(buf) = mode {
        let line = Line::from(vec![
            Span::styled(
                ":",
                Style::default()
                    .fg(theme.cmd.into())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(buf, Style::default().fg(theme.fg.into())),
            Span::styled("▌", Style::default().fg(theme.cmd.into())),
        ]);
        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(theme.cursor_bg.into())),
            area,
        );
    } else {
        frame.render_widget(
            Paragraph::new("").style(Style::default().bg(theme.bg.into())),
            area,
        );
    }
}
