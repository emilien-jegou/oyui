use crate::config::UiTheme;
use ratatui::{style::Style, text::Span};

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

pub fn prepare_code_spans<'a>(
    spans: Vec<Span<'a>>,
    hscroll: usize,
    code_col_width: usize,
    content_len: usize,
    theme: &UiTheme,
) -> Vec<Span<'a>> {
    if content_len == 0 {
        return spans;
    }

    let has_left = hscroll > 0;
    let has_right = content_len > hscroll + code_col_width;

    // 1. Skip first `hscroll` characters
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

    // 2. Truncate to `code_col_width` characters
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

    // 3. Apply indicators dimly using theme.dim without bleeding style to adjacent text
    let indicator_style = Style::default().fg(theme.dim.into());

    if has_left && !capped_spans.is_empty() {
        let first_span = capped_spans.remove(0);
        let mut chars = first_span.content.chars();
        chars.next(); // Drop the first char
        let rest: String = chars.collect();

        let symbol = if has_right && capped_spans.is_empty() && rest.is_empty() {
            "↔"
        } else {
            "˂"
        };

        // Insert the dimly styled indicator
        capped_spans.insert(
            0,
            Span::styled(symbol, first_span.style.patch(indicator_style)),
        );

        // Put the rest of the text back with its original syntax highlighting style
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
                "↔"
            } else {
                "˃"
            };

            // Push the prefix back with its original syntax highlighting style
            if !prefix.is_empty() {
                capped_spans.push(Span::styled(prefix, last_span.style));
            }

            // Push the dimly styled indicator at the very end
            capped_spans.push(Span::styled(symbol, last_span.style.patch(indicator_style)));
        }
    }

    capped_spans
}
