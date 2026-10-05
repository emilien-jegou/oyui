use crate::app::ui_state::{Message, MessageLevel};
use crate::app::{merge_stats, App, CommandMode};
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

    let theme = app.theme.read().ui.clone();

    let diff_summary = merge_stats::diff_summary(&app.tree);

    // Scoped: the guard must be released before anything below re-locks the
    // tree, because a queued writer turns a second read into a deadlock.
    {
        let tree_guard = app.tree.read();
        // 1. Draw the underlying view first
        app.ui.lock().draw(
            frame,
            view_area,
            &tree_guard,
            &app.cache,
            app.base_path.as_ref(),
            diff_summary,
            &theme,
            &app.color_mode,
        );
    }

    // 2. Draw the config error on top of the view if it exists
    if let Some(ref err) = *app.config.error.read() {
        crate::view::config_error::draw(frame, view_area, err, &theme);
    }

    // 3. Draw the keybinding help overlay on top of everything.
    if app.ui.lock().help.is_some() {
        let entries = app.config.keybinds.entries();
        let current = app.ui.lock().current;
        let mut scroll = app.ui.lock().help.as_ref().map_or(0, |h| h.scroll);
        crate::view::keybinds::draw(frame, view_area, &entries, current, &mut scroll, &theme);
        if let Some(help) = app.ui.lock().help.as_mut() {
            help.scroll = scroll;
        }
    }

    let cmd_mode = app.ui.lock().command_mode.clone();

    // Expire stale script notifications before rendering them.
    let message = {
        let mut ui = app.ui.lock();
        if ui
            .message
            .as_ref()
            .is_some_and(|m| m.expires_at <= std::time::Instant::now())
        {
            ui.message = None;
        }
        ui.message.clone()
    };
    let status = app.ui.lock().status.clone();

    let view = app.ui.lock().current;
    let hint_override = app
        .ui
        .lock()
        .hint_formats
        .get(match view {
            ViewKind::Tree => "tree",
            ViewKind::File => "file",
        })
        .cloned();

    draw_hint_bar(
        frame,
        hint_area,
        &cmd_mode,
        &view,
        hint_override.as_deref(),
        &theme,
    );
    draw_command_bar(
        frame,
        cmd_area,
        &cmd_mode,
        message.as_ref(),
        &status,
        &theme,
    );

    if let CommandMode::ConfirmMerge = cmd_mode {
        let stats = merge_stats::merge_stats(&app.tree, &app.cache);
        crate::view::confirm_window::draw(frame, &theme, stats);
    }
}

fn draw_hint_bar(
    frame: &mut Frame,
    area: Rect,
    mode: &CommandMode,
    view: &ViewKind,
    script_override: Option<&str>,
    theme: &UiTheme,
) {
    // A script-defined format only applies to the normal mode; modal hints are
    // fixed because they describe mandatory controls.
    let script_hints = script_override
        .filter(|_| matches!(mode, CommandMode::Normal))
        .map(parse_hint_format);

    let hints = match script_hints {
        Some(hints) => hints,
        None => match mode {
            CommandMode::Normal => match view {
                ViewKind::Tree => vec![
                    ("j/k".to_string(), "move".to_string()),
                    ("h/l".to_string(), "close/open".to_string()),
                    ("space".to_string(), "stage".to_string()),
                    ("i".to_string(), "invert".to_string()),
                    (":".to_string(), "cmd".to_string()),
                    ("enter".to_string(), "merge".to_string()),
                    ("q".to_string(), "quit".to_string()),
                ],
                ViewKind::File => vec![
                    ("j/k".to_string(), "move".to_string()),
                    ("n/N".to_string(), "hunks".to_string()),
                    ("space".to_string(), "stage".to_string()),
                    ("z".to_string(), "unfold".to_string()),
                    ("s".to_string(), "split".to_string()),
                    ("t".to_string(), "line".to_string()),
                    ("h/esc".to_string(), "back".to_string()),
                    ("enter".to_string(), "merge".to_string()),
                    ("q".to_string(), "quit".to_string()),
                ],
            },
            CommandMode::Active(_) => vec![
                ("enter".to_string(), "run".to_string()),
                ("esc".to_string(), "cancel".to_string()),
            ],
            CommandMode::ConfirmMerge => vec![
                ("enter".to_string(), "confirm".to_string()),
                ("q/esc".to_string(), "cancel".to_string()),
            ],
        },
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

/// Parses `"key=desc key=desc"`; `_` in a description becomes a space.
fn parse_hint_format(s: &str) -> Vec<(String, String)> {
    s.split_whitespace()
        .filter_map(|token| token.split_once('='))
        .map(|(k, v)| (k.to_string(), v.replace('_', " ")))
        .collect()
}

fn draw_command_bar(
    frame: &mut Frame,
    area: Rect,
    mode: &CommandMode,
    message: Option<&Message>,
    status: &str,
    theme: &UiTheme,
) {
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
    } else if let Some(message) = message {
        let color = match message.level {
            MessageLevel::Info => theme.fg,
            MessageLevel::Warn => theme.partial,
            MessageLevel::Error => theme.del_fg,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {} ", message.text),
                Style::default().fg(color.into()),
            )))
            .style(Style::default().bg(theme.bg.into())),
            area,
        );
    } else if !status.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {status}"),
                Style::default().fg(theme.dim.into()),
            )))
            .style(Style::default().bg(theme.bg.into())),
            area,
        );
    } else {
        frame.render_widget(
            Paragraph::new("").style(Style::default().bg(theme.bg.into())),
            area,
        );
    }
}
