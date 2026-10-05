//! Conflict-resolution overlay for merge sessions.

use crate::app::ui_state::ResolveState;
use crate::config::UiTheme;
use crate::diff::Side;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Draws the current conflict with ours/base/theirs sections.
pub fn draw(frame: &mut Frame, area: Rect, state: &ResolveState, theme: &UiTheme) {
    frame.render_widget(Clear, area);

    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " o=ours  t=theirs  b=both  u=unset  n/N or j/k=move  enter=write  esc=close",
            Style::default().fg(theme.dim.into()),
        )),
        Line::from(""),
    ];

    if let Some(conflict) = state.current() {
        let choice = state.choices.get(state.cursor).copied().flatten();
        push_section(
            &mut lines,
            "OURS",
            &conflict.ours,
            choice == Some(Side::Ours),
            theme.staged,
            theme,
        );
        if let Some(base) = &conflict.base {
            push_section(&mut lines, "BASE", base, false, theme.partial, theme);
        }
        push_section(
            &mut lines,
            "THEIRS",
            &conflict.theirs,
            choice == Some(Side::Theirs),
            theme.del_fg,
            theme,
        );
    }

    let title = format!(
        " Conflicts {}/{} ({} resolved) ",
        state.cursor + 1,
        state.count().max(1),
        state.resolved_count()
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.partial.into())
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(theme.partial.into()))
        .style(Style::default().bg(theme.bg.into()));

    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(theme.bg.into()))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn push_section(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    body: &[String],
    selected: bool,
    color: crate::config::theme::Color,
    theme: &UiTheme,
) {
    let marker = if selected { "● " } else { "  " };
    lines.push(Line::from(Span::styled(
        format!("{marker}{label}"),
        Style::default()
            .fg(color.into())
            .add_modifier(Modifier::BOLD),
    )));
    for line in body {
        lines.push(Line::from(Span::styled(
            format!("    {line}"),
            Style::default().fg(theme.fg.into()),
        )));
    }
    lines.push(Line::from(""));
}
