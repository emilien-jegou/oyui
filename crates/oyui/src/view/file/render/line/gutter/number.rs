use crate::config::UiTheme;
use crate::view::file::utils::colors::safe_lerp_color;
use ratatui::{style::Style, text::Span, widgets::Cell};
use typed_builder::TypedBuilder;

#[derive(TypedBuilder)]
pub struct GutterNumber<'a> {
    pub idx: usize,
    pub is_selected: bool,
    #[builder(default)]
    pub is_dimmed: bool,
    pub is_add: bool,
    pub is_del: bool,
    pub is_staged: bool,
    pub theme: &'a UiTheme,
    #[builder(default)]
    pub custom_style: Option<Style>,
}

impl<'a> GutterNumber<'a> {
    pub fn compute_style(&self) -> Style {
        let mut line_num_style = if self.is_selected {
            if self.is_staged && (self.is_add || self.is_del) {
                Style::default()
                    .bg(safe_lerp_color(&self.theme.cursor_bg, &self.theme.partial, 0.2).into())
                    .fg(self.theme.fg.into())
            } else {
                Style::default()
                    .bg(self.theme.cursor_bg.into())
                    .fg(self.theme.fg.into())
            }
        } else if self.is_staged && (self.is_add || self.is_del) {
            Style::default()
                .bg(safe_lerp_color(&self.theme.bg, &self.theme.partial, 0.1).into())
                .fg(self.theme.partial.into())
        } else {
            let mut fg = self.theme.dim.clone();
            if self.is_dimmed {
                fg = safe_lerp_color(&fg, &self.theme.bg, 0.4);
            }
            Style::default().bg(self.theme.bg.into()).fg(fg.into())
        };

        if let Some(override_style) = self.custom_style {
            line_num_style = line_num_style.patch(override_style);
        }

        line_num_style
    }

    pub fn render(&self) -> Cell<'a> {
        self.render_with_style(self.compute_style())
    }

    pub fn render_with_style(&self, style: Style) -> Cell<'a> {
        let line_num_span = Span::styled(format!("{:>4} ", self.idx + 1), style);
        Cell::from(line_num_span).style(style)
    }
}
