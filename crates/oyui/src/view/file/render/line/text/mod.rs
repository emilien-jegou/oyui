pub mod spans_wrapper;

use crate::{
    config::UiTheme,
    diff::InlineChange,
    view::file::{
        render::style::{to_tui_style, LineBgCalculator},
        utils::colors::safe_lerp_color,
    },
};
use ratatui::{style::Style, text::Span, widgets::Cell};
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

        let trailing_space_start_byte = self.content.trim_end_matches(' ').len();
        let has_special_chars =
            self.content.contains('\t') || trailing_space_start_byte < self.content.len();

        let tab_str = &self.theme.char_tab;
        let trailing_space_str = &self.theme.char_trailing_space;
        let tab_color = self.theme.char_tab_fg.into();
        let trailing_space_color = self.theme.char_trailing_space_fg.into();

        let mut push_slice =
            |slice: &str, slice_start_byte: usize, style: Style, has_inline: bool| {
                let slice_has_special = has_special_chars
                    && (slice.contains('\t')
                        || (slice_start_byte + slice.len() > trailing_space_start_byte));

                if !char_by_char && !has_inline && !slice_has_special {
                    let mut final_style = style;
                    if !has_inline && !char_by_char {
                        final_style = final_style.bg(bg_calc.get_bg(visual_x).into());
                    }
                    row_spans.push(Span::styled(slice.to_string(), final_style));
                    visual_x += slice.chars().count();
                    return;
                }

                let mut current_string = String::new();
                let mut current_bg = None;
                let mut current_fg = None;

                let mut char_byte_offset = 0;

                for c in slice.chars() {
                    let char_len = c.len_utf8();
                    let abs_byte = slice_start_byte + char_byte_offset;
                    char_byte_offset += char_len;

                    let target_bg = bg_calc.get_bg(visual_x);
                    let is_tab = c == '\t';
                    let is_trailing_space = c == ' ' && abs_byte >= trailing_space_start_byte;

                    let (display_str, target_fg) = if is_tab {
                        (Some(tab_str.as_str()), Some(tab_color))
                    } else if is_trailing_space {
                        (
                            Some(trailing_space_str.as_str()),
                            Some(trailing_space_color),
                        )
                    } else {
                        (None, None)
                    };

                    let effective_fg = target_fg.or(style.fg);

                    if Some(target_bg) != current_bg || current_fg != effective_fg {
                        if !current_string.is_empty() {
                            let mut s = style;
                            if let Some(bg) = current_bg {
                                s = s.bg(bg.into());
                            }
                            if let Some(fg) = current_fg {
                                s = s.fg(fg.into());
                            }
                            row_spans.push(Span::styled(current_string.clone(), s));
                            current_string.clear();
                        }
                        current_bg = Some(target_bg);
                        current_fg = effective_fg;
                    }

                    if let Some(s) = display_str {
                        current_string.push_str(s);
                    } else {
                        current_string.push(c);
                    }

                    visual_x += 1;
                }

                if !current_string.is_empty() {
                    let final_bg = current_bg.unwrap_or_else(|| bg_calc.get_bg(visual_x));
                    let mut s = style.bg(final_bg.into());
                    if let Some(fg) = current_fg {
                        s = s.fg(fg);
                    }
                    row_spans.push(Span::styled(current_string, s));
                }
            };

        let inline_bg = if !self.is_staged {
            if self.is_selected {
                self.theme.cursor_bg
            } else {
                self.theme.bg
            }
        } else if self.is_selected {
            if self.is_add {
                safe_lerp_color(&self.theme.add_bg, &self.theme.add_fg, 0.4)
            } else {
                safe_lerp_color(&self.theme.del_bg, &self.theme.del_fg, 0.4)
            }
        } else if self.is_add {
            safe_lerp_color(&self.theme.add_bg, &self.theme.add_fg, 0.2)
        } else {
            safe_lerp_color(&self.theme.del_bg, &self.theme.del_fg, 0.2)
        };

        let tokens: Vec<(Style, &str)> = if !self.is_del {
            self.syntax_opt
                .and_then(|lines| lines.get(self.idx))
                .filter(|t| !t.is_empty())
                .map(|t| {
                    t.iter()
                        .map(|(s, text)| (to_tui_style(*s), text.as_str()))
                        .collect()
                })
                .unwrap_or_else(|| vec![(Style::default().fg(self.theme.fg.into()), self.content)])
        } else {
            vec![(Style::default().fg(self.theme.fg.into()), self.content)]
        };

        let mut current_byte = 0;

        for (mut base_style, text) in tokens {
            let text_start = current_byte;
            let text_end = text_start + text.len();

            if self.is_add {
                base_style = base_style.fg(self.theme.add_fg.into());
            }
            if self.is_del {
                base_style = base_style.fg(self.theme.del_fg.into());
            }

            let mut token_offset = 0;

            while token_offset < text.len() {
                let abs_byte = text_start + token_offset;
                let active_hl = self
                    .inline_highlights
                    .iter()
                    .find(|h| h.byte_range.contains(&abs_byte));

                let prev_offset = token_offset;

                if let Some(hl) = active_hl {
                    let hl_end_in_token =
                        (hl.byte_range.end.saturating_sub(text_start)).min(text.len());
                    if let Some(slice) = text.get(token_offset..hl_end_in_token) {
                        push_slice(slice, abs_byte, base_style.bg(inline_bg.into()), true);
                    } else {
                        push_slice(&text[token_offset..], abs_byte, base_style, false);
                        break;
                    }
                    token_offset = hl_end_in_token;
                } else {
                    let next_hl_start = self
                        .inline_highlights
                        .iter()
                        .map(|h| h.byte_range.start)
                        .filter(|&start| start > abs_byte)
                        .min()
                        .unwrap_or(text_end);

                    let next_hl_in_token =
                        (next_hl_start.saturating_sub(text_start)).min(text.len());
                    if let Some(slice) = text.get(token_offset..next_hl_in_token) {
                        push_slice(slice, abs_byte, base_style, false);
                    } else {
                        push_slice(&text[token_offset..], abs_byte, base_style, false);
                        break;
                    }
                    token_offset = next_hl_in_token;
                }

                if token_offset <= prev_offset {
                    break;
                }
            }
            // src/view/file/render/text/mod.rs

            current_byte = text_end;
        }

        let code_col_width = (self.area_width as usize).saturating_sub(6);

        // Pad the row spans up to the scrolled viewport width using the custom background calculator
        let current_char_count = visual_x - self.visual_x_offset;
        let target_len = self.hscroll + code_col_width;
        if current_char_count < target_len {
            let base_style = if self.is_add {
                Style::default().fg(self.theme.add_fg.into())
            } else if self.is_del {
                Style::default().fg(self.theme.del_fg.into())
            } else {
                Style::default().fg(self.theme.fg.into())
            };

            for _ in current_char_count..target_len {
                let bg_color = bg_calc.get_bg(visual_x);
                row_spans.push(Span::styled(" ", base_style.bg(bg_color.into())));
                visual_x += 1;
            }
        }

        let content_len: usize = row_spans.iter().map(|s| s.content.chars().count()).sum();

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
