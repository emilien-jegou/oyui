use crate::config::UiTheme;
use crate::view::file::render::line::text::spans_wrapper::slice_spans;
use crate::view::file::utils::colors::safe_lerp_color;
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::{Cell, Row},
};

pub fn render_separator<'a>(
    hidden_count: usize,
    next_old: Option<usize>,
    next_new: Option<usize>,
    is_selected: bool,
    theme: &UiTheme,
    hscroll: usize,
) -> Row<'a> {
    let mut style = Style::default()
        .bg(safe_lerp_color(&theme.bg, &theme.dir, 0.1).into())
        .fg(safe_lerp_color(&theme.dim, &theme.dir, 0.5).into());
    if is_selected {
        style = style.bg(theme.cursor_bg.into()).fg(theme.fg.into());
    }

    let mut spans = vec![];
    if let (Some(old), Some(new)) = (next_old, next_new) {
        let fgc = if is_selected {
            theme.fg.into()
        } else {
            safe_lerp_color(&theme.dim, &theme.dir, 0.8)
        };
        // Removed leading space before '@@'
        spans.push(Span::styled(
            format!("@@ -{} +{} @@  ", old + 1, new + 1),
            style.fg(fgc.into()),
        ));
    }
    // Removed leading/trailing spaces around '⋯'
    spans.push(Span::styled(
        format!("⋯ {} hidden lines ⋯", hidden_count),
        style,
    ));

    let final_spans = if hscroll > 0 {
        slice_spans(spans, hscroll)
    } else {
        spans
    };

    Row::new(vec![
        Cell::from(" ").style(style), // Indicator Column
        Cell::from("  ⋮  ").style(style.fg(if is_selected {
            // Line Number Column
            theme.fg.into()
        } else {
            theme.dim.into()
        })),
        Cell::from("  ").style(style),                    // Sign Column
        Cell::from(Line::from(final_spans)).style(style), // Text Column
    ])
    .style(style)
}
