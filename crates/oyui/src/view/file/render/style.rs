use crate::{
    config::{theme::Color, LineHighlightMode, UiTheme},
    view::file::utils::colors::{darken_color, lighten_color, safe_lerp_color},
};
use ratatui::style::Style;

/// Conflict accent: a first-class theme color like `add_fg`/`del_fg`
/// (derived as the staged/deleted midpoint, overridable per theme).
pub fn conflict_orange(theme: &UiTheme) -> Color {
    theme.conflict_fg
}

/// The underlay color behind conflict blocks, blended by the configured
/// opacity. `conflict_bg` overrides the derived tint of the theme bg.
///
/// Only fold frames use it now; conflict lines themselves are plain rows.
pub fn conflict_underlay(theme: &UiTheme) -> Color {
    let raw = theme.conflict_bg.unwrap_or_else(|| {
        safe_lerp_color(
            &theme.bg,
            &conflict_orange(theme),
            0.3 * theme.file_conflict_highlight_opacity as f32,
        )
    });
    safe_lerp_color(
        &theme.bg,
        &raw,
        theme.file_conflict_highlight_opacity as f32,
    )
}

/// True when the conflict underlay highlight is enabled.
fn conflict_highlight_on(theme: &UiTheme) -> bool {
    !matches!(theme.file_conflict_highlight, LineHighlightMode::None)
}

pub fn get_line_style(
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    is_staged: bool,
    is_conflict: bool,
    use_gradient: bool,
    theme: &UiTheme,
    is_preview: bool,
) -> Style {
    // Conflict content lines keep their add/del colors (including the green
    // gradient); only marker lines (which carry no +/- sign) fall back to
    // the plain background with the orange accent fg. The hover preview wash
    // still applies on top.
    let is_add_or_del = is_add || is_del;

    // We only use an uncolored row background (theme.bg or cursor_bg) if the file change highlight
    // is a Gradient and gradients are enabled. This allows the char-by-char gradient to transition
    // cleanly to the standard background.
    let use_grad_change = use_gradient
        && is_add_or_del
        && matches!(theme.file_change_highlight, LineHighlightMode::Gradient(_));

    if use_grad_change {
        let bg = if is_selected {
            theme.cursor_bg.into()
        } else {
            theme.bg.into()
        };
        let mut style = Style::default().bg(bg);
        if is_add {
            style = style.fg(theme.add_fg.into());
        } else {
            style = style.fg(theme.del_fg.into());
        }
        if !is_staged {
            style = style.fg(theme.dim.into());
        }
        return style;
    }

    // Determine solid background modes.
    // If use_gradient is false, treat both Solid and Gradient settings as Solid.
    let has_change_solid = if use_gradient {
        theme.file_change_highlight == LineHighlightMode::Solid
    } else {
        matches!(
            theme.file_change_highlight,
            LineHighlightMode::Solid | LineHighlightMode::Gradient(_)
        )
    };

    let has_staged_solid = is_staged
        && if use_gradient {
            theme.file_staged_highlight == LineHighlightMode::Solid
        } else {
            matches!(
                theme.file_staged_highlight,
                LineHighlightMode::Solid | LineHighlightMode::Gradient(_)
            )
        };

    let add_bg = safe_lerp_color(
        &theme.add_bg,
        &theme.bg,
        1.0 - theme.file_change_highlight_opacity as f32,
    );
    let del_bg = safe_lerp_color(
        &theme.del_bg,
        &theme.bg,
        1.0 - theme.file_change_highlight_opacity as f32,
    );
    let accent_bg_solid = safe_lerp_color(
        &theme.bg,
        &theme.partial,
        theme.file_staged_highlight_opacity as f32,
    );

    let mut style = Style::default().fg(theme.fg.into());
    let mut bg_col = theme.bg;

    if is_add_or_del {
        if is_add {
            style = style.fg(theme.add_fg.into());
        } else {
            style = style.fg(theme.del_fg.into());
        }

        let change_bg = if is_add { add_bg } else { del_bg };

        if has_staged_solid {
            // When staged solid highlight is active, blend staged color and change color 50/50
            bg_col = safe_lerp_color(&accent_bg_solid, &change_bg, 0.5);
        } else if has_change_solid {
            bg_col = change_bg;
        }
    }

    // Apply cursor layer on top of the calculated background.
    // Blend with cursor_bg, then lighten or darken based on the theme.
    if is_selected {
        bg_col = safe_lerp_color(&theme.cursor_bg, &bg_col, 0.3);
    }

    style = style.bg(bg_col.into());

    if !is_staged && is_add_or_del {
        style = style.fg(theme.dim.into());
        // Do not override back to theme.bg if a solid change highlight is active
        if !is_selected && !has_change_solid {
            style = style.bg(theme.bg.into());
        }
    }

    // The hovered side's marker takes the frame wash; conflict rows
    // otherwise sit on the plain background.
    if is_conflict && conflict_highlight_on(theme) && is_preview && !is_selected {
        style = style.bg(conflict_underlay(theme).into());
    } else if is_preview && !is_selected && !is_conflict {
        let tint = safe_lerp_color(
            &theme.bg,
            &theme.partial,
            theme.file_staged_highlight_opacity as f32,
        );
        style = style.bg(tint.into());
    }

    style
}

pub fn to_tui_style(style: syntect::highlighting::Style) -> Style {
    Style::default()
        .fg(Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b).into())
}

pub struct LineBgCalculator {
    grad1_width: f32,
    grad2_width: f32,
    frame_grad_width: f32,
    frame_grad_on: bool,
    use_grad_change: bool,
    use_gradient_change_solid: bool,
    use_staged_grad: bool,
    use_staged_solid: bool,
    use_conflict: bool,
    is_selected: bool,
    is_preview: bool,
    is_staged: bool,
    is_add_or_del: bool,

    // Base/neutral colors without cursor blending
    bg: Color,
    cursor_bg: Color,
    neutral_change_bg: Color,
    neutral_accent_bg_grad: Color,
    frame_accent: Color,
}

impl LineBgCalculator {
    pub fn new(
        is_add: bool,
        is_del: bool,
        is_selected: bool,
        is_staged: bool,
        is_conflict: bool,
        use_gradient: bool,
        area_width: u16,
        theme: &UiTheme,
        is_preview: bool,
    ) -> Self {
        let area_width = area_width.max(10);
        let grad1_width = match theme.file_change_highlight {
            LineHighlightMode::Gradient(pct) => (area_width as f64 * pct).max(1.0) as f32,
            _ => 1.0,
        };
        let grad2_width = match theme.file_staged_highlight {
            LineHighlightMode::Gradient(pct) => (area_width as f64 * pct).max(1.0) as f32,
            _ => 1.0,
        };
        let use_conflict = is_conflict && conflict_highlight_on(theme);
        // Frame-style wash for the hovered marker: same gradient logic as
        // staged hunks, tuned-down orange.
        let frame_grad_on = use_gradient
            && use_conflict
            && matches!(
                theme.file_conflict_highlight,
                LineHighlightMode::Gradient(_)
            );
        let frame_grad_width = match theme.file_conflict_highlight {
            LineHighlightMode::Gradient(pct) => (area_width as f64 * pct).max(1.0) as f32,
            _ => 1.0,
        };

        // Conflict content lines keep their change/staged washes; markers
        // (no +/- sign) naturally fall back to the plain background.
        let is_add_or_del = is_add || is_del;
        let use_grad_change = use_gradient
            && is_add_or_del
            && matches!(theme.file_change_highlight, LineHighlightMode::Gradient(_));

        // If use_gradient is false, treat both Solid and Gradient as Solid
        let use_gradient_change_solid = if use_gradient {
            is_add_or_del && theme.file_change_highlight == LineHighlightMode::Solid
        } else {
            is_add_or_del
                && matches!(
                    theme.file_change_highlight,
                    LineHighlightMode::Solid | LineHighlightMode::Gradient(_)
                )
        };

        let use_staged_grad = use_gradient
            && is_staged
            && is_add_or_del
            && matches!(theme.file_staged_highlight, LineHighlightMode::Gradient(_));

        // If use_gradient is false, treat both Solid and Gradient as Solid
        let use_staged_solid = is_staged
            && is_add_or_del
            && if use_gradient {
                theme.file_staged_highlight == LineHighlightMode::Solid
            } else {
                matches!(
                    theme.file_staged_highlight,
                    LineHighlightMode::Solid | LineHighlightMode::Gradient(_)
                )
            };

        let raw_change_bg = if is_add { &theme.add_bg } else { &theme.del_bg };
        let neutral_change_bg = safe_lerp_color(
            &raw_change_bg,
            &theme.bg,
            1.0 - theme.file_change_highlight_opacity as f32,
        );
        let neutral_accent_bg_grad = safe_lerp_color(
            &theme.bg,
            &theme.partial,
            theme.file_staged_highlight_opacity as f32,
        );
        let frame_accent = conflict_underlay(theme);

        Self {
            grad1_width,
            grad2_width,
            frame_grad_width,
            frame_grad_on,
            use_grad_change,
            use_gradient_change_solid,
            use_staged_grad,
            use_staged_solid,
            use_conflict,
            is_selected,
            is_preview,
            is_staged,
            is_add_or_del,
            bg: theme.bg,
            cursor_bg: theme.cursor_bg,
            neutral_change_bg,
            neutral_accent_bg_grad,
            frame_accent,
        }
    }

    pub fn get_bg(&self, visual_x: usize) -> Color {
        // The hovered side's marker takes the frame wash; anything else
        // previews like a staged hunk.
        if self.is_preview && !self.is_selected && self.use_conflict {
            if self.frame_grad_on {
                let t = (visual_x as f32 / self.frame_grad_width).clamp(0.0, 1.0);
                return safe_lerp_color(&self.frame_accent, &self.bg, t);
            }
            return self.frame_accent;
        }
        let base = self.base_bg(visual_x);
        if self.is_preview && !self.is_selected {
            safe_lerp_color(&base, &self.neutral_accent_bg_grad, 0.45)
        } else {
            base
        }
    }

    fn base_bg(&self, visual_x: usize) -> Color {
        let base_bg_neutral = if self.use_grad_change {
            let t1 = (visual_x as f32 / self.grad1_width).clamp(0.0, 1.0);
            safe_lerp_color(&self.neutral_change_bg, &self.bg, t1)
        } else if self.use_gradient_change_solid {
            self.neutral_change_bg
        } else {
            self.bg
        };

        let mut final_bg_neutral = base_bg_neutral;

        if self.is_staged && self.is_add_or_del {
            if self.use_staged_grad {
                let t2 = (visual_x as f32 / self.grad2_width).clamp(0.0, 1.0);
                final_bg_neutral =
                    safe_lerp_color(&self.neutral_accent_bg_grad, &base_bg_neutral, t2);
            } else if self.use_staged_solid {
                final_bg_neutral =
                    safe_lerp_color(&self.neutral_accent_bg_grad, &base_bg_neutral, 0.3);
            }
        }

        if self.is_selected {
            // Apply cursor/selection background on top of the final background color.
            // Blend with cursor_bg, then lighten or darken based on the theme.
            let blend = safe_lerp_color(&self.cursor_bg, &final_bg_neutral, 0.2);
            if self.bg.is_dark() {
                lighten_color(&blend, 0.08)
            } else {
                darken_color(&blend, 0.08)
            }
        } else {
            final_bg_neutral
        }
    }

    pub fn char_by_char(&self) -> bool {
        self.use_grad_change
            || self.use_staged_grad
            || (self.is_preview && self.use_conflict && self.frame_grad_on)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_colors::TerminalColorMode;
    use crate::theme::ansi_default_theme;

    /// Conflict content lines keep their add/del fg; markers carry no sign
    /// so they fall back to the plain fg. Only the hover preview adds the
    /// orange underlay.
    #[test]
    fn conflict_lines_keep_change_colors() {
        let theme = ansi_default_theme(&TerminalColorMode::NoColor);
        let style = get_line_style(true, false, false, true, true, false, &theme, false);
        assert_eq!(style.fg, Some(theme.add_fg.into()));
    }

    /// The hover preview puts the frame wash on the hovered side's marker.
    #[test]
    fn conflict_preview_tints_orange() {
        use crate::config::theme::Color;
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.bg = Color::Rgb(20, 20, 20);
        // Non-preview conflict content keeps its change styling, preview
        // swaps in the conflict underlay.
        let plain = get_line_style(false, false, false, false, true, false, &theme, false);
        let preview = get_line_style(false, false, false, false, true, false, &theme, true);
        assert_eq!(plain.bg, Some(theme.bg.into()));
        assert_eq!(preview.bg, Some(conflict_underlay(&theme).into()));
    }

    /// The conflict accent follows the theme instead of a fixed RGB.
    #[test]
    fn conflict_orange_tracks_theme() {
        let mut theme = ansi_default_theme(&TerminalColorMode::NoColor);
        theme.conflict_fg = Color::Rgb(1, 2, 3);
        assert_eq!(conflict_orange(&theme), Color::Rgb(1, 2, 3));
    }
}
