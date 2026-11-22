pub mod spans_wrapper;

use crate::{
    config::UiTheme,
    diff::InlineChange,
    view::file::render::style::{to_tui_style, word_layer, LineBgCalculator},
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

#[derive(Clone, Copy)]
pub struct TextRenderer<'a> {
    pub content: &'a str,
    pub idx: usize,
    pub is_add: bool,
    pub is_del: bool,
    pub is_selected: bool,
    pub is_staged: bool,
    pub is_conflict: bool,
    pub is_preview: bool,
    pub conflict_fg: Option<crate::config::theme::Color>,
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
    /// The span sequence for the line (before the wrapping Cell stage); kept
    /// separate from [`Self::render`] so tests can inspect the painted
    /// layers directly.
    fn render_line(self) -> ratatui::text::Line<'a> {
        let mut row_spans = vec![];
        let mut visual_x = self.visual_x_offset;

        let bg_calc = LineBgCalculator::new(
            self.is_add,
            self.is_del,
            self.is_selected,
            self.is_staged,
            self.is_conflict,
            self.use_gradient,
            self.area_width,
            self.theme,
            self.is_preview,
        );

        let char_by_char = bg_calc.char_by_char();

        // Word-level layer for changed rows: painted inside inline ranges
        // (the spans' inner bg/fg), on top of the row background.
        let word = word_layer(self.is_add, self.theme);

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

                    let row_bg = bg_calc.get_bg(visual_x);
                    // Chars inside a changed span take the word layer painted
                    // over the row bg; everything else keeps the row bg.
                    let target_bg = match (&word, has_inline) {
                        (Some(w), true) => w.bg_over(row_bg),
                        _ => row_bg,
                    };
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

                    let element_fg = match (&word, has_inline) {
                        // Word fg override wins inside changed spans; `None`
                        // keeps the token's syntax fg.
                        (Some(w), true) => w.fg.map(|f| f.into()).or(style.fg),
                        _ => style.fg,
                    };

                    let effective_fg = target_fg.or(element_fg);

                    if Some(target_bg) != current_bg || current_fg != effective_fg {
                        if !current_string.is_empty() {
                            let mut s = style;
                            if let Some(bg) = current_bg {
                                s = s.bg(bg.into());
                            }
                            if let Some(fg) = current_fg {
                                s = s.fg(fg);
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
            // Deletion rows receive the old side's tokens as `syntax_opt`
            // (rows.rs picks the side); without tokens, the plain fg.
            self.syntax_opt
                .and_then(|lines| lines.get(self.idx))
                .filter(|t| !t.is_empty())
                .map(|t| {
                    t.iter()
                        .map(|(s, text)| (to_tui_style(*s), text.as_str()))
                        .collect()
                })
                .unwrap_or_else(|| vec![(Style::default().fg(self.theme.fg.into()), self.content)])
        };

        let mut current_byte = 0;

        for (mut base_style, text) in tokens {
            let text_start = current_byte;
            let text_end = text_start + text.len();

            // Outer fg: override only when the theme asks
            // (`add_text_fg`/`del_text_fg`); `None` keeps the token's syntax
            // fg. The conflict accent keeps its precedence above both.
            if self.is_add {
                if let Some(fg) = self.theme.add_text_fg {
                    base_style = base_style.fg(fg.into());
                }
            }
            if self.is_del {
                if let Some(fg) = self.theme.del_text_fg {
                    base_style = base_style.fg(fg.into());
                }
            }
            if let Some(fg) = self.conflict_fg {
                base_style = base_style.fg(fg.into());
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
                        push_slice(slice, abs_byte, base_style, true);
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

        final_line
    }

    pub fn render(self) -> Cell<'a> {
        if !self.config.enabled {
            return Cell::from("");
        }

        let final_line = self.render_line();

        let mut text_cell = Cell::from(final_line).style(self.row_style);
        if let Some(override_style) = self.config.style {
            text_cell = text_cell.style(self.row_style.patch(override_style));
        }

        text_cell
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::theme::{Color, LineHighlightMode};
    use crate::terminal_colors::TerminalColorMode;
    use crate::theme::ansi_default_theme;

    type RColor = ratatui::prelude::Color;

    /// `config::Color` into its ratatui form (explicit helper keeps the
    /// expected-value assertions below readable).
    fn col(c: crate::config::theme::Color) -> RColor {
        c.into()
    }

    /// Reverse of [`col`] for colors the renderer painted into spans.
    fn to_config(c: RColor) -> crate::config::theme::Color {
        match c {
            RColor::Rgb(r, g, b) => crate::config::theme::Color::Rgb(r, g, b),
            other => panic!("rgb expected: {other:?}"),
        }
    }

    /// Builds the renderer for one line of content with the given inline
    /// ranges. Staged so no preview or selection washes interfere.
    fn renderer<'a>(
        theme: &'a UiTheme,
        content: &'a str,
        inline_highlights: &'a [InlineChange],
        syntax_opt: Option<&'a Vec<Vec<(syntect::highlighting::Style, String)>>>,
    ) -> TextRenderer<'a> {
        TextRenderer {
            content,
            idx: 0,
            is_add: true,
            is_del: false,
            is_selected: false,
            is_staged: true,
            is_conflict: false,
            is_preview: false,
            conflict_fg: None,
            inline_highlights,
            syntax_opt,
            area_width: 40,
            use_gradient: false,
            theme,
            hscroll: 0,
            row_style: Style::default(),
            visual_x_offset: 0,
            config: TextConfig {
                enabled: true,
                style: None,
            },
        }
    }

    /// Expands the painted spans into one (bg, fg) pair per char column.
    fn char_styles(line: ratatui::text::Line<'_>) -> Vec<(Option<RColor>, Option<RColor>)> {
        line.spans
            .iter()
            .flat_map(|span| {
                let (bg, fg) = (span.style.bg, span.style.fg);
                span.content.chars().map(move |_| (bg, fg))
            })
            .collect()
    }

    /// Chars inside an inline range take the word layer (bg lerped over the
    /// row bg, fg kept from the token); chars outside keep the row bg.
    #[test]
    fn word_bg_paints_inside_inline_range_only() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.bg = Color::Rgb(10, 10, 10);
        theme.add_bg = Color::Rgb(0, 200, 0);
        theme.add_fg = Color::Rgb(255, 0, 255);
        // Defaults: word layer Solid + opacity 0.4, no fg overrides.

        let highlights = vec![InlineChange {
            byte_range: 1..3, // "bc" of "abcd"
        }];

        let line = renderer(&theme, "abcd", &highlights, None).render_line();
        let styles = char_styles(line);

        let (outer_bg, outer_fg) = styles[0];
        assert!(outer_bg.is_some());

        // Inner chars painted the word layer over the row bg; outer chars
        // keep the row bg.
        let word_style = word_layer(true, &theme).expect("word layer");
        let (inner_bg, inner_fg) = styles[1];
        assert_eq!(
            inner_bg,
            Some(col(word_style.bg_over(to_config(outer_bg.expect("row bg")))))
        );
        assert_eq!(styles[2].0, inner_bg, "both inner chars share the word bg");
        assert_ne!(inner_bg, outer_bg, "the inner layer must be visible");

        // Word fg defaults to `None`: the token fg (theme.fg without syntax
        // tokens) survives on both sides of the boundary.
        let expected_fg = Some(col(theme.fg));
        assert_eq!(outer_fg, expected_fg);
        assert_eq!(inner_fg, expected_fg);
    }

    /// `add_text_fg` overrides the outer fg; `add_word_fg` overrides the
    /// fg inside changed spans.
    #[test]
    fn fg_overrides_honor_theme_fields() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.add_fg = Color::Rgb(255, 0, 255);
        theme.add_text_fg = Some(Color::Rgb(1, 2, 3));
        theme.add_word_fg = Some(Color::Rgb(4, 5, 6));

        let highlights = vec![InlineChange { byte_range: 1..3 }];

        let line = renderer(&theme, "abcd", &highlights, None).render_line();
        let styles = char_styles(line);

        assert_eq!(styles[0].1, Some(col(Color::Rgb(1, 2, 3))));
        assert_eq!(styles[1].1, Some(col(Color::Rgb(4, 5, 6))));
        assert_eq!(styles[3].1, Some(col(Color::Rgb(1, 2, 3))));
    }

    /// Without any inline ranges the word layer never paints.
    #[test]
    fn whole_line_changes_show_no_word_layer() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.add_bg = Color::Rgb(0, 200, 0);

        let line = renderer(&theme, "abcd", &[], None).render_line();
        let styles = char_styles(line);

        assert!(
            styles.iter().all(|(bg, _)| *bg == styles[0].0),
            "uniform row bg without inline ranges"
        );
    }

    /// `LineHighlightMode::None` on the word layer turns the inner layer off
    /// entirely (the legacy look).
    #[test]
    fn word_layer_none_keeps_plain_inner() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.add_bg = Color::Rgb(0, 200, 0);
        theme.file_change_word_highlight = LineHighlightMode::None;

        let highlights = vec![InlineChange { byte_range: 1..3 }];

        let line = renderer(&theme, "abcd", &highlights, None).render_line();
        let styles = char_styles(line);

        assert!(
            styles.iter().all(|(bg, _)| *bg == styles[0].0),
            "word layer disabled: uniform row bg"
        );
    }

    /// Without an fg override, the token's syntax fg survives the row.
    #[test]
    fn syntax_fg_survives_by_default() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.add_fg = Color::Rgb(255, 0, 255);
        theme.fg = Color::Rgb(240, 240, 240);

        // One token covering the whole line with its own fg.
        let syn_style = syntect::highlighting::Style {
            foreground: syntect::highlighting::Color {
                r: 11,
                g: 22,
                b: 33,
                a: 255,
            },
            ..Default::default()
        };
        let tokens = vec![vec![(syn_style, "abcd".to_string())]];

        let line = renderer(&theme, "abcd", &[], Some(&tokens)).render_line();
        let styles = char_styles(line);

        assert_eq!(styles[0].1, Some(col(Color::Rgb(11, 22, 33))));
        // Regression guard for the old force-override: the row add_fg must
        // NOT replace the token fg when no override is set.
        assert_ne!(styles[0].1, Some(col(theme.add_fg)));
    }
}
