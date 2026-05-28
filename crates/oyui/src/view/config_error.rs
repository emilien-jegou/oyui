use crate::config::UiTheme;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, area: Rect, error_msg: &str, theme: &UiTheme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Configuration Error ")
        .title_style(
            Style::default()
                .fg(theme.del_fg.into())
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(theme.del_fg.into()))
        .style(Style::default().bg(theme.bg.into()));

    let text = parse_ansi_text(error_msg, theme);

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().bg(theme.bg.into()))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Helper stateful parser that converts standard ANSI formatting bytes into a styled Ratatui Text representation.
fn parse_ansi_text(ansi_str: &str, theme: &UiTheme) -> Text<'static> {
    let mut lines = Vec::new();

    for line in ansi_str.lines() {
        let mut spans = Vec::new();
        let mut current_style = Style::default();
        let mut chars = line.chars().peekable();
        let mut current_text = String::new();

        let flush_text = |text: &mut String, spans: &mut Vec<Span<'static>>, style: Style| {
            if !text.is_empty() {
                spans.push(Span::styled(text.clone(), style));
                text.clear();
            }
        };

        while let Some(c) = chars.next() {
            if c == '\x1B' {
                if chars.peek() == Some(&'[') {
                    chars.next(); // Consume '['

                    // Read digits/parameters until termination byte 'm'
                    let mut seq = String::new();
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc == 'm' {
                            break;
                        }
                        seq.push(nc);
                    }

                    // Flush text processed under the old style
                    flush_text(&mut current_text, &mut spans, current_style);

                    // Update style using standard parser helper
                    current_style = apply_ansi_sequence(current_style, &seq, theme);
                } else {
                    current_text.push(c);
                }
            } else {
                current_text.push(c);
            }
        }

        flush_text(&mut current_text, &mut spans, current_style);
        lines.push(Line::from(spans));
    }

    Text::from(lines)
}

fn apply_ansi_sequence(mut style: Style, seq: &str, theme: &UiTheme) -> Style {
    if seq.is_empty() || seq == "0" {
        return Style::default();
    }

    let codes: Vec<&str> = seq.split(';').collect();
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            "0" => style = Style::default(),
            "1" => style = style.add_modifier(Modifier::BOLD),
            "2" => style = style.add_modifier(Modifier::DIM),
            "3" => style = style.add_modifier(Modifier::ITALIC),
            "4" => style = style.add_modifier(Modifier::UNDERLINED),
            "9" => style = style.add_modifier(Modifier::CROSSED_OUT),

            // Standard Foreground Colors
            "30" => style = style.fg(Color::Black),
            "31" => style = style.fg(theme.del_fg.into()),
            "32" => style = style.fg(theme.staged.into()),
            "33" => style = style.fg(theme.partial.into()),
            "34" => style = style.fg(theme.dir.into()),
            "35" => style = style.fg(theme.cmd.into()),
            "36" => style = style.fg(theme.dir.into()),
            "37" => style = style.fg(theme.fg.into()),

            // High-intensity standard Foreground Colors
            "90" => style = style.fg(Color::DarkGray),
            "91" => style = style.fg(theme.del_fg.into()),
            "92" => style = style.fg(theme.staged.into()),
            "93" => style = style.fg(theme.partial.into()),
            "94" => style = style.fg(theme.dir.into()),
            "95" => style = style.fg(theme.cmd.into()),
            "96" => style = style.fg(theme.dir.into()),
            "97" => style = style.fg(theme.fg.into()),

            // 256 or RGB Foreground Colors
            "38" if i + 2 < codes.len() && codes[i + 1] == "5" => {
                if let Ok(color_idx) = codes[i + 2].parse::<u8>() {
                    style = style.fg(Color::Indexed(color_idx));
                }
                i += 2;
            }
            "38" if i + 4 < codes.len() && codes[i + 1] == "2" => {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    codes[i + 2].parse::<u8>(),
                    codes[i + 3].parse::<u8>(),
                    codes[i + 4].parse::<u8>(),
                ) {
                    style = style.fg(Color::Rgb(r, g, b));
                }
                i += 4;
            }

            _ => {}
        }
        i += 1;
    }

    style
}
