pub mod indicator;

pub mod number;
pub mod sign;

use crate::{config::UiTheme, diff::HunkMarker};
use ratatui::{style::Style, widgets::Cell};
use typed_builder::TypedBuilder;

#[derive(Clone, Copy, Debug)]
pub struct GutterConfig {
    pub show_indicator: bool,
    pub show_number: bool,
    pub show_sign: bool,
    pub indicator_style: Option<Style>,
    pub number_style: Option<Style>,
    pub sign_style: Option<Style>,
}

impl Default for GutterConfig {
    fn default() -> Self {
        Self {
            show_indicator: true,
            show_number: true,
            show_sign: true,
            indicator_style: None,
            number_style: None,
            sign_style: None,
        }
    }
}


#[derive(TypedBuilder)]
pub struct GutterRenderer<'a> {
    pub config: GutterConfig,
    pub idx: usize,
    #[builder(default)]
    pub is_add: bool,
    #[builder(default)]
    pub is_del: bool,
    #[builder(default)]
    pub is_selected: bool,
    #[builder(default)]
    pub is_staged: bool,
    pub mode: HunkMarker,
    #[builder(default)]
    pub use_gradient: bool,
    pub area_width: u16,
    pub row_style: Style,
    pub theme: &'a UiTheme,
}

impl<'a> GutterRenderer<'a> {
    pub fn render(self) -> Vec<Cell<'a>> {
        let mut cells = vec![];

        let number_renderer = number::GutterNumber::builder()
            .idx(self.idx)
            .is_selected(self.is_selected)
            .is_add(self.is_add)
            .is_del(self.is_del)
            .is_staged(self.is_staged)
            .theme(self.theme)
            .custom_style(self.config.number_style)
            .build();

        let computed_number_style = number_renderer.compute_style();

        // 1. Vertical Indicator
        if self.config.show_indicator {
            cells.push(
                indicator::GutterIndicator::builder()
                    .is_add(self.is_add)
                    .is_del(self.is_del)
                    .is_staged(self.is_staged)
                    .mode(self.mode)
                    .bg_color(computed_number_style.bg)
                    .theme(self.theme)
                    .custom_style(self.config.indicator_style)
                    .build()
                    .render(),
            );
        } else {
            cells.push(Cell::from(""));
        }

        // 2. Line Number
        if self.config.show_number {
            cells.push(number_renderer.render_with_style(computed_number_style));
        } else {
            cells.push(Cell::from(""));
        }

        // 3. Change Sign
        if self.config.show_sign {
            cells.push(
                sign::GutterSign::builder()
                    .is_add(self.is_add)
                    .is_del(self.is_del)
                    .is_staged(self.is_staged)
                    .is_selected(self.is_selected)
                    .use_gradient(self.use_gradient)
                    .area_width(self.area_width)
                    .row_style(self.row_style)
                    .theme(self.theme)
                    .custom_style(self.config.sign_style)
                    .build()
                    .render(),
            );
        } else {
            cells.push(Cell::from(""));
        }

        cells
    }
}
