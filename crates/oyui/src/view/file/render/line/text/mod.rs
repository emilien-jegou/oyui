pub mod spans_wrapper;

use crate::{
    config::UiTheme,
    diff::InlineChange,
    view::file::{
        render::style::{to_tui_style, LineBgCalculator},
        utils::colors::lerp_color,
    },
};
use ratatui::{
    style::{Color, Style},
    text::Span,
    widgets::Cell,
};
use spans_wrapper::SpansWrapper;

#[derive(Clone, Copy, Debug)]
pub struct TextConfig {
    pub enabled: bool,
    pub style: Option<Style>,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            style: None,
        }
    }
}

pub struct TextRenderer<'a> {
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
    pub row_style: Style,
    pub visual_x_offset: usize,
    pub config: TextConfig,
}

impl<'a> TextRenderer<'a> {
    pub fn render(self) -> Cell<'a> {
        if !self.config.enabled {
            return Cell::from("");
        }

        let mut row_spans = vec![];
        let mut visual_x = self.visual_x_offset;

        let bg_calc = LineBgCalculator::new(
            self.is_add,
            self.is_del,
            self.is_selected,
            self.is_staged,
            self.use_gradient,
            self.area_width,
            self.theme,
        );

        let char_by_char = bg_calc.char_by_char();

        let mut push_slice = |slice: &str, style: Style, has_inline: bool| {
            if !char_by_char || has_inline {
                let mut final_style = style;
                if !has_inline && !char_by_char {
                    final_style = final_style.bg(bg_calc.get_bg(visual_x));
                }
                row_spans.push(Span::styled(slice.to_string(), final_style));
                visual_x += slice.chars().count();
                return;
            }

            let mut current_string = String::new();
            let mut current_bg = None;

            for c in slice.chars() {
                let target_bg = bg_calc.get_bg(visual_x);

                if Some(target_bg) != current_bg {
                    if !current_string.is_empty() {
                        row_spans.push(Span::styled(
                            current_string.clone(),
                            style.bg(current_bg.unwrap()),
                        ));
                        current_string.clear();
                    }
                    current_bg = Some(target_bg);
                }
                current_string.push(c);
                visual_x += 1;
            }

            if !current_string.is_empty() {
                let final_bg = current_bg.unwrap_or_else(|| bg_calc.get_bg(visual_x));
                row_spans.push(Span::styled(current_string, style.bg(final_bg)));
            }
        };

        let inline_bg: Color = if !self.is_staged {
            if self.is_selected {
                self.theme.cursor_bg.into()
            } else {
                self.theme.bg.into()
            }
        } else if self.is_selected {
            if self.is_add {
                lerp_color(self.theme.add_bg.into(), self.theme.add_fg.into(), 0.4)
            } else {
                lerp_color(self.theme.del_bg.into(), self.theme.del_fg.into(), 0.4)
            }
        } else if self.is_add {
            lerp_color(self.theme.add_bg.into(), self.theme.add_fg.into(), 0.2)
        } else {
            lerp_color(self.theme.del_bg.into(), self.theme.del_fg.into(), 0.2)
        };

        let crate::config::theme::Color::Rgb(fg_r, fg_g, fg_b) = self.theme.fg;
        let fallback_style = syntect::highlighting::Style {
            foreground: syntect::highlighting::Color { r: fg_r, g: fg_g, b: fg_b, a: 255 },
            background: syntect::highlighting::Color::WHITE,
            font_style: syntect::highlighting::FontStyle::empty(),
        };
        let fallback_tokens = vec![(fallback_style, self.content.to_string())];

        let tokens = if !self.is_del {
            self.syntax_opt
                .and_then(|lines| lines.get(self.idx))
                .filter(|t| !t.is_empty())
                .unwrap_or(&fallback_tokens)
        } else {
            &fallback_tokens
        };

        let mut current_byte = 0;

        for (syn_style, text) in tokens {
            let text_start = current_byte;
            let text_end = text_start + text.len();

            let mut base_style = to_tui_style(*syn_style);

            if self.is_add {
                base_style = base_style.fg(self.theme.add_fg.into());
            }
            if self.is_del {
                base_style = base_style.fg(self.theme.del_fg.into());
            }

            let mut token_offset = 0;

            while token_offset < text.len() {
                let abs_byte = text_start + token_offset;
                let active_hl = self.inline_highlights
                    .iter()
                    .find(|h| h.byte_range.contains(&abs_byte));

                let prev_offset = token_offset;

                if let Some(hl) = active_hl {
                    let hl_end_in_token = (hl.byte_range.end.saturating_sub(text_start)).min(text.len());
                    if let Some(slice) = text.get(token_offset..hl_end_in_token) {
                        push_slice(slice, base_style.bg(inline_bg), true);
                    } else {
                        push_slice(&text[token_offset..], base_style, false);
                        break;
                    }
                    token_offset = hl_end_in_token;
                } else {
                    let next_hl_start = self.inline_highlights
                        .iter()
                        .map(|h| h.byte_range.start)
                        .filter(|&start| start > abs_byte)
                        .min()
                        .unwrap_or(text_end);

                    let next_hl_in_token = (next_hl_start.saturating_sub(text_start)).min(text.len());
                    if let Some(slice) = text.get(token_offset..next_hl_in_token) {
                        push_slice(slice, base_style, false);
                    } else {
                        push_slice(&text[token_offset..], base_style, false);
                        break;
                    }
                    token_offset = next_hl_in_token;
                }

                if token_offset <= prev_offset {
                    break;
                }
            }
            current_byte = text_end;
        }

        let content_len = self.content.chars().count();
        let code_col_width = (self.area_width as usize).saturating_sub(6);

        let final_line = SpansWrapper {
            spans: row_spans,
            hscroll: self.hscroll,
            code_col_width,
            content_len,
            theme: self.theme,
        }
        .wrap();

        let mut text_cell = Cell::from(final_line).style(self.row_style);
        if let Some(override_style) = self.config.style {
            text_cell = text_cell.style(self.row_style.patch(override_style));
        }

        text_cell
    }
}
