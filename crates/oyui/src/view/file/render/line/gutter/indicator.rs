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
    #[builder(default)]
    pub is_hunk_split: bool,
    pub bg_color: Option<Color>,
    pub theme: &'a UiTheme,
    #[builder(default)]
    pub custom_style: Option<Style>,
}

impl<'a> GutterIndicator<'a> {
    pub fn compute_style(&self) -> Style {
        let mut fg: Color = if self.is_staged {
            self.theme.partial.into()
        } else {
            self.theme.dim.into()
        };

        // Apply fallback logic: Use the specified color, otherwise defaults to the bar's staging color
        if self.is_hunk_split {
            if let Some(custom_color) = self.theme.char_hunk_split_color {
                fg = custom_color.into();
            }
        }

        let mut sign_style = Style::default()
            .fg(fg)
            .bg(self.bg_color.unwrap_or(Color::Reset));

        if let Some(override_style) = self.custom_style {
            sign_style = sign_style.patch(override_style);
        }

        sign_style
    }

    pub fn render(&self) -> Cell<'a> {
        let sign_char = if self.is_hunk_split {
            self.theme.char_hunk_split.as_str()
        } else if self.is_add || self.is_del {
            self.theme.char_indicator.as_str()
        } else {
            " "
        };
        let style = self.compute_style();

        Cell::from(Span::styled(sign_char, style)).style(style)
    }
}
