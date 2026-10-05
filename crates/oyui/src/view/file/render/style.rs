use crate::{
    config::{theme::Color, LineHighlightMode, UiTheme},
    view::file::utils::colors::{darken_color, lighten_color, safe_lerp_color, underlay_of},
};
use ratatui::style::Style;

/// The underlay color behind conflict blocks, blended by the configured
/// opacity. `conflict_bg` overrides the derived darken/lighten of the theme bg.
pub fn conflict_underlay(theme: &UiTheme) -> Color {
    let raw = theme
        .conflict_bg
        .unwrap_or_else(|| underlay_of(&theme.bg).unwrap_or(theme.cursor_bg));
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
) -> Style {
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

    // Conflict lines keep their change foreground but sit on the underlay.
    if is_conflict && conflict_highlight_on(theme) {
        let mut bg = conflict_underlay(theme);
        if is_selected {
            bg = safe_lerp_color(&theme.cursor_bg, &bg, 0.3);
        }
        style = style.bg(bg.into());
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
    grad_conflict_width: f32,
    use_grad_change: bool,
    use_gradient_change_solid: bool,
    use_staged_grad: bool,
    use_staged_solid: bool,
    use_conflict: bool,
    use_conflict_grad: bool,
    is_selected: bool,
    is_staged: bool,
    is_add_or_del: bool,

    // Base/neutral colors without cursor blending
    bg: Color,
    cursor_bg: Color,
    neutral_change_bg: Color,
    neutral_accent_bg_grad: Color,
    conflict_bg: Color,
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
        let grad_conflict_width = match theme.file_conflict_highlight {
            LineHighlightMode::Gradient(pct) => (area_width as f64 * pct).max(1.0) as f32,
            _ => 1.0,
        };

        let use_conflict = is_conflict && conflict_highlight_on(theme);
        let use_conflict_grad = use_gradient
            && use_conflict
            && matches!(
                theme.file_conflict_highlight,
                LineHighlightMode::Gradient(_)
            );
        let conflict_bg = conflict_underlay(theme);

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

        Self {
            grad1_width,
            grad2_width,
            grad_conflict_width,
            use_grad_change,
            use_gradient_change_solid,
            use_staged_grad,
            use_staged_solid,
            use_conflict,
            use_conflict_grad,
            is_selected,
            is_staged,
            is_add_or_del,
            bg: theme.bg,
            cursor_bg: theme.cursor_bg,
            neutral_change_bg,
            neutral_accent_bg_grad,
            conflict_bg,
        }
    }

    pub fn get_bg(&self, visual_x: usize) -> Color {
        if self.use_conflict {
            let base = if self.use_conflict_grad {
                let t = (visual_x as f32 / self.grad_conflict_width).clamp(0.0, 1.0);
                safe_lerp_color(&self.conflict_bg, &self.bg, t)
            } else {
                self.conflict_bg
            };
            return if self.is_selected {
                safe_lerp_color(&self.cursor_bg, &base, 0.2)
            } else {
                base
            };
        }

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
        self.use_grad_change || self.use_staged_grad || self.use_conflict_grad
    }
}
