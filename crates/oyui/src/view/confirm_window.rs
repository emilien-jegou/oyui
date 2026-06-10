use crate::config::UiTheme;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};

pub fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    Layout::vertical([Constraint::Length(height)])
        .flex(ratatui::layout::Flex::Center)
        .split(
            Layout::horizontal([Constraint::Length(width)])
                .flex(ratatui::layout::Flex::Center)
                .split(r)[0],
        )[0]
}

pub fn draw(
    frame: &mut Frame,
    theme: &UiTheme,
    merge_stats: (
        (usize, usize, usize, usize, usize),
        (usize, usize, usize, usize, usize),
    ),
) {
    let confirm_area = centered_rect(70, 11, frame.area());
    frame.render_widget(Clear, confirm_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm Merge ")
        .border_style(Style::default().fg(theme.cmd.into()))
        .style(Style::default().bg(theme.bg.into()));

    let inner_area = block.inner(confirm_area);
    frame.render_widget(block, confirm_area);

    let main_layout = Layout::vertical([
        Constraint::Length(1), // Top spacing
        Constraint::Length(1), // Instruction text
        Constraint::Length(1), // Empty
        Constraint::Length(4), // Table Section
        Constraint::Min(0),    // Empty spacer
        Constraint::Length(1), // Footer
    ])
    .split(inner_area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "You are about to execute a merge.",
            Style::default().fg(theme.fg.into()),
        )]))
        .alignment(Alignment::Center),
        main_layout[1],
    );

    let (left, right) = merge_stats;

    let header = Row::new(vec![
        Cell::from(""),
        Cell::from(
            Line::from(Span::styled(
                "Left (Staged)",
                Style::default()
                    .fg(theme.fg.into())
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        ),
        Cell::from(
            Line::from(Span::styled(
                "Right (Unstaged)",
                Style::default()
                    .fg(theme.fg.into())
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        ),
    ])
    .bottom_margin(1);

    let files_row = Row::new(vec![
        Cell::from(
            Line::from(Span::styled("Files", Style::default().fg(theme.dim.into())))
                .alignment(Alignment::Right),
        ),
        Cell::from(
            Line::from(vec![
                Span::styled(
                    format!("{}A ", left.0),
                    Style::default().fg(theme.add_fg.into()),
                ),
                Span::styled(
                    format!("{}D ", left.1),
                    Style::default().fg(theme.del_fg.into()),
                ),
                Span::styled(
                    format!("{}M", left.2),
                    Style::default().fg(theme.partial.into()),
                ),
            ])
            .alignment(Alignment::Center),
        ),
        Cell::from(
            Line::from(vec![
                Span::styled(
                    format!("{}A ", right.0),
                    Style::default().fg(theme.add_fg.into()),
                ),
                Span::styled(
                    format!("{}D ", right.1),
                    Style::default().fg(theme.del_fg.into()),
                ),
                Span::styled(
                    format!("{}M", right.2),
                    Style::default().fg(theme.partial.into()),
                ),
            ])
            .alignment(Alignment::Center),
        ),
    ]);

    let lines_row = Row::new(vec![
        Cell::from(
            Line::from(Span::styled("Lines", Style::default().fg(theme.dim.into())))
                .alignment(Alignment::Right),
        ),
        Cell::from(
            Line::from(vec![
                Span::styled(
                    format!("+{} ", left.3),
                    Style::default()
                        .fg(theme.add_fg.into())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/ ", Style::default().fg(theme.dim.into())),
                Span::styled(
                    format!("-{}", left.4),
                    Style::default()
                        .fg(theme.del_fg.into())
                        .add_modifier(Modifier::BOLD),
                ),
            ])
            .alignment(Alignment::Center),
        ),
        Cell::from(
            Line::from(vec![
                Span::styled(
                    format!("+{} ", right.3),
                    Style::default()
                        .fg(theme.add_fg.into())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("/ ", Style::default().fg(theme.dim.into())),
                Span::styled(
                    format!("-{}", right.4),
                    Style::default()
                        .fg(theme.del_fg.into())
                        .add_modifier(Modifier::BOLD),
                ),
            ])
            .alignment(Alignment::Center),
        ),
    ]);

    let table = Table::new(
        vec![files_row, lines_row],
        [
            Constraint::Length(8),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ],
    )
    .header(header)
    .column_spacing(1);

    frame.render_widget(table, main_layout[3]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Press ", Style::default().fg(theme.dim.into())),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme.cmd.into())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to confirm, ", Style::default().fg(theme.dim.into())),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(theme.cmd.into())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to cancel.", Style::default().fg(theme.dim.into())),
        ]))
        .alignment(Alignment::Center),
        main_layout[5],
    );
}
