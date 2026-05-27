pub mod gutter;
pub mod text;

use super::style::get_line_style;
use crate::{config::UiTheme, diff::InlineChange};
use gutter::{GutterConfig, GutterRenderer};
use ratatui::{
    layout::Constraint, style::Stylize, widgets::{Block, Borders, Row, Table}
};
use text::{TextConfig, TextRenderer};

pub fn build_line_table<'a>(rows: Vec<Row<'a>>, theme: &UiTheme) -> Table<'a> {
    Table::new(
        rows,
        [
            Constraint::Length(1), // Indicator
            Constraint::Length(5), // Line numbers
            Constraint::Length(2), // Sign (+/-)
            Constraint::Min(0),    // Main code content
        ],
    )
    .column_spacing(0)
    .block(Block::default().borders(Borders::NONE).bg(theme.bg))
}

pub struct LineRenderer<'a> {
    pub content: &'a str,
    pub idx: usize,
    pub is_add: bool,
    pub is_del: bool,
    pub is_selected: bool,
    pub is_staged: bool,
    pub inline_highlights: &'a [InlineChange],
    pub syntax_opt: Option<&'a Vec<Vec<(syntect::highlighting::Style, String)>>>,
    pub area_width: u16,
    pub use_gradient: bool,
    pub theme: &'a UiTheme,
    pub hscroll: usize,
    
    pub gutter_config: GutterConfig,
    pub text_config: TextConfig,
}

impl<'a> LineRenderer<'a> {
    pub fn render(self) -> Row<'a> {
        let row_style = get_line_style(
            self.is_add,
            self.is_del,
            self.is_selected,
            self.is_staged,
            self.use_gradient,
            self.theme,
        );

        let mut row_cells = GutterRenderer {
            config: self.gutter_config,
            idx: self.idx,
            is_add: self.is_add,
            is_del: self.is_del,
            is_selected: self.is_selected,
            is_staged: self.is_staged,
            use_gradient: self.use_gradient,
            area_width: self.area_width,
            row_style,
            theme: self.theme,
        }
        .render();

        let visual_x_offset = if self.gutter_config.show_sign { 2 } else { 0 };

        let text_cell = TextRenderer {
            content: self.content,
            idx: self.idx,
            is_add: self.is_add,
            is_del: self.is_del,
            is_selected: self.is_selected,
            is_staged: self.is_staged,
            inline_highlights: self.inline_highlights,
            syntax_opt: self.syntax_opt,
            area_width: self.area_width,
            use_gradient: self.use_gradient,
            theme: self.theme,
            hscroll: self.hscroll,
            row_style,
            visual_x_offset,
            config: self.text_config,
        }
        .render();

        row_cells.push(text_cell);

        Row::new(row_cells).style(row_style)
    }
}
