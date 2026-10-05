//! Keybinding-help overlay listing every configured binding.

use crate::actions::keybinds::{KeybindEntry, KeybindMode, View};
use crate::config::UiTheme;
use crate::view::ViewKind;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const KEY_COLUMN: usize = 20;

/// Draws the scrollable keybinding list, marking the active view.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    entries: &[KeybindEntry],
    current: ViewKind,
    scroll: &mut usize,
    theme: &UiTheme,
) {
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Press ? or Esc to close",
        Style::default().fg(theme.dim.into()),
    )));
    lines.push(Line::from(""));

    let sections = [
        (KeybindMode::Global, "Global", None),
        (KeybindMode::View(View::File), "File", Some(ViewKind::File)),
        (KeybindMode::View(View::Tree), "Tree", Some(ViewKind::Tree)),
    ];

    for (mode, title, view) in sections {
        let marker = match view {
            Some(v) if v == current => "  (current)",
            _ => "",
        };
        lines.push(Line::from(Span::styled(
            format!(" {title}{marker}"),
            Style::default()
                .fg(theme.cmd.into())
                .add_modifier(Modifier::BOLD),
        )));

        for entry in entries.iter().filter(|e| e.mode == mode) {
            let mut keys = entry.keys.join(", ");
            if keys.len() < KEY_COLUMN {
                keys.push_str(&" ".repeat(KEY_COLUMN - keys.len()));
            }
            let label = entry.labels.join(" + ");
            lines.push(Line::from(vec![
                Span::styled(format!("   {keys}"), Style::default().fg(theme.fg.into())),
                Span::styled(label, Style::default().fg(theme.dim.into())),
            ]));
        }
        lines.push(Line::from(""));
    }

    let max_scroll = lines.len().saturating_sub(area.height as usize);
    *scroll = (*scroll).min(max_scroll);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keybindings ")
        .title_style(
            Style::default()
                .fg(theme.fg.into())
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(theme.cmd.into()))
        .style(Style::default().bg(theme.bg.into()));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .style(Style::default().bg(theme.bg.into()))
        .scroll((*scroll as u16, 0));

    frame.render_widget(paragraph, area);
}
