use crate::config::UiTheme;
use crate::view::file::render::style::LineBgCalculator;
use crate::view::file::utils::colors::{darken_color, desaturate_color, is_dark, lighten_color};
use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::Cell,
};
use typed_builder::TypedBuilder;

#[derive(TypedBuilder)]
pub struct GutterSign<'a> {
    pub is_add: bool,
    pub is_del: bool,
    pub is_staged: bool,
    pub is_selected: bool,
    pub use_gradient: bool,
    pub area_width: u16,
    pub row_style: Style,
    pub theme: &'a UiTheme,
    #[builder(default)]
    pub custom_style: Option<Style>,
}

impl<'a> GutterSign<'a> {
    pub fn compute_base_style(&self) -> Style {
        let is_theme_dark = is_dark(self.theme.bg.into());
        let mut prefix_style = self.row_style;

        if self.is_add {
            let mut col = self.theme.add_fg.into();
            if !self.is_staged {
                col = desaturate_color(col, 0.60);
                col = if is_theme_dark {
                    darken_color(col, 0.30)
                } else {
                    lighten_color(col, 0.30)
                };
            }
            prefix_style = prefix_style.fg(col);
        } else if self.is_del {
            let mut col = self.theme.del_fg.into();
            if !self.is_staged {
                col = desaturate_color(col, 0.60);
                col = if is_theme_dark {
                    darken_color(col, 0.30)
                } else {
                    lighten_color(col, 0.30)
                };
            }
            prefix_style = prefix_style.fg(col);
        }

        if let Some(override_style) = self.custom_style {
            prefix_style = prefix_style.patch(override_style);
        }

        prefix_style
    }

    pub fn render(&self) -> Cell<'a> {
        let prefix = if self.is_add {
            self.theme.char_add_sign.as_str()
        } else if self.is_del {
            self.theme.char_del_sign.as_str()
        } else {
            "  "
        };
        let prefix_style = self.compute_base_style();

        let bg_calc = LineBgCalculator::new(
            self.is_add,
            self.is_del,
            self.is_selected,
            self.is_staged,
            self.use_gradient,
            self.area_width,
            self.theme,
        );

        if !bg_calc.char_by_char() {
            let bg = bg_calc.get_bg(0);
            let final_style = prefix_style.bg(bg);
            return Cell::from(Span::styled(prefix, final_style)).style(self.row_style);
        }

        // Apply gradient character-by-character to the sign
        let mut spans = vec![];
        for (i, c) in prefix.chars().enumerate() {
            let bg = bg_calc.get_bg(i);
            spans.push(Span::styled(c.to_string(), prefix_style.bg(bg)));
        }

        Cell::from(Line::from(spans)).style(self.row_style)
    }
}
