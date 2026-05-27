use crate::config::UiTheme;
use ratatui::{
    style::{Color, Style},
    text::Span,
    widgets::Cell,
};
use typed_builder::TypedBuilder;

#[derive(TypedBuilder)]
pub struct GutterIndicator<'a> {
    pub is_add: bool,
    pub is_del: bool,
    pub is_staged: bool,
    pub bg_color: Option<Color>,
    pub theme: &'a UiTheme,
    #[builder(default)]
    pub custom_style: Option<Style>,
}

impl<'a> GutterIndicator<'a> {
    pub fn compute_style(&self) -> Style {
        let fg: Color = if self.is_staged {
            self.theme.partial.into()
        } else {
            self.theme.dim.into()
        };

        let mut sign_style = Style::default()
            .fg(fg)
            .bg(self.bg_color.unwrap_or(Color::Reset));

        if let Some(override_style) = self.custom_style {
            sign_style = sign_style.patch(override_style);
        }

        sign_style
    }

    pub fn render(&self) -> Cell<'a> {
        let sign_char = if self.is_add || self.is_del {
            "▎"
        } else {
            " "
        };
        let style = self.compute_style();

        Cell::from(Span::styled(sign_char, style)).style(style)
    }
}
