use crate::config::UiTheme;
use ratatui::{
    style::Style,
    text::{Line, Span},
};

pub fn slice_spans<'a>(spans: Vec<Span<'a>>, skip: usize) -> Vec<Span<'a>> {
    if skip == 0 {
        return spans;
    }
    let mut result = Vec::new();
    let mut skipped = 0;
    for span in spans {
        if skipped >= skip {
            result.push(span);
            continue;
        }
        let chars_count = span.content.chars().count();
        if skipped + chars_count <= skip {
            skipped += chars_count;
            continue;
        }
        let chars_to_skip = skip - skipped;
        let new_content: String = span.content.chars().skip(chars_to_skip).collect();
        result.push(Span::styled(new_content, span.style));
        skipped = skip;
    }
    result
}

pub struct SpansWrapper<'a> {
    pub spans: Vec<Span<'a>>,
    pub hscroll: usize,
    pub code_col_width: usize,
    pub content_len: usize,
    pub theme: &'a UiTheme,
}

impl<'a> SpansWrapper<'a> {
    pub fn wrap(self) -> Line<'a> {
        let Self {
            spans,
            hscroll,
            code_col_width,
            content_len,
            theme,
        } = self;

        if content_len == 0 {
            return Line::from(spans);
        }

        let has_left = hscroll > 0;
        let has_right = content_len > hscroll + code_col_width;

        let mut skipped_spans = Vec::new();
        let mut skipped = 0;
        for span in spans {
            let chars_count = span.content.chars().count();
            if skipped + chars_count <= hscroll {
                skipped += chars_count;
                continue;
            }
            if skipped < hscroll {
                let chars_to_skip = hscroll - skipped;
                let new_content: String = span.content.chars().skip(chars_to_skip).collect();
                skipped_spans.push(Span::styled(new_content, span.style));
                skipped = hscroll;
            } else {
                skipped_spans.push(span);
            }
        }

        let mut capped_spans = Vec::new();
        let mut current_width = 0;
        for span in skipped_spans {
            let chars_count = span.content.chars().count();
            if current_width >= code_col_width {
                break;
            }
            if current_width + chars_count > code_col_width {
                let allowed = code_col_width - current_width;
                let new_content: String = span.content.chars().take(allowed).collect();
                capped_spans.push(Span::styled(new_content, span.style));
                current_width = code_col_width;
            } else {
                current_width += chars_count;
                capped_spans.push(span);
            }
        }

        let indicator_style =
            Style::default().fg(theme.char_scroll_fg.into());

        if has_left && !capped_spans.is_empty() {
            let first_span = capped_spans.remove(0);
            let mut chars = first_span.content.chars();
            chars.next();
            let rest: String = chars.collect();

            let symbol = if has_right && capped_spans.is_empty() && rest.is_empty() {
                &theme.char_scroll_both
            } else {
                &theme.char_scroll_left
            };

            capped_spans.insert(
                0,
                Span::styled(symbol.clone(), first_span.style.patch(indicator_style)),
            );

            if !rest.is_empty() {
                capped_spans.insert(1, Span::styled(rest, first_span.style));
            }
        }

        if has_right && !capped_spans.is_empty() {
            let last_span = capped_spans.pop().unwrap();
            let chars_count = last_span.content.chars().count();
            if chars_count > 0 {
                let prefix: String = last_span.content.chars().take(chars_count - 1).collect();
                let symbol = if has_left && capped_spans.is_empty() && prefix.is_empty() {
                    &theme.char_scroll_both
                } else {
                    &theme.char_scroll_right
                };

                if !prefix.is_empty() {
                    capped_spans.push(Span::styled(prefix, last_span.style));
                }

                capped_spans.push(Span::styled(
                    symbol.clone(),
                    last_span.style.patch(indicator_style),
                ));
            }
        }

        Line::from(capped_spans)
    }
}
