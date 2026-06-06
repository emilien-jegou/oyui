use crate::{config::UiTheme, diff::HunkMarker};
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
    pub mode: HunkMarker,
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

        match self.mode {
            HunkMarker::LineToggle => {
                if let Some(custom_color) = self.theme.char_line_split_color {
                    fg = custom_color.into();
                }
            }
            HunkMarker::HunkSplit => {
                if let Some(custom_color) = self.theme.char_hunk_split_color {
                    fg = custom_color.into();
                }
            }
            HunkMarker::None => {}
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
        let sign_char = match self.mode {
            HunkMarker::LineToggle => self.theme.char_line_split.as_str(),
            HunkMarker::HunkSplit => self.theme.char_hunk_split.as_str(),
            HunkMarker::None => {
                if self.is_add || self.is_del {
                    self.theme.char_indicator.as_str()
                } else {
                    " "
                }
            }
        };
        let style = self.compute_style();

        Cell::from(Span::styled(sign_char, style)).style(style)
    }
}
