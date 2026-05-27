use super::super::utils::colors::lerp_color;
use crate::config::{LineHighlightMode, UiTheme};
use ratatui::style::{Color, Style};

pub fn get_line_style(
    is_add: bool,
    is_del: bool,
    is_selected: bool,
    is_staged: bool,
    use_gradient: bool,
    theme: &UiTheme,
) -> Style {
    let use_grad_change = use_gradient
        && is_staged
        && (is_add || is_del)
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
        return style;
    }

    let has_highlight =
        use_gradient && is_staged && theme.file_change_highlight != LineHighlightMode::None;

    let add_bg = lerp_color(
        theme.add_bg.into(),
        theme.bg.into(),
        1.0 - theme.file_change_highlight_opacity as f32,
    );
    let del_bg = lerp_color(
        theme.del_bg.into(),
        theme.bg.into(),
        1.0 - theme.file_change_highlight_opacity as f32,
    );

    let mut style = if is_selected {
        Style::default()
            .bg(theme.cursor_bg.into())
            .fg(theme.fg.into())
    } else {
        Style::default().bg(theme.bg.into()).fg(theme.fg.into())
    };

    if is_add {
        style = style.fg(theme.add_fg.into());
        if has_highlight {
            let bg_col = if is_selected {
                lerp_color(theme.cursor_bg.into(), add_bg, 0.6)
            } else {
                add_bg
            };
            style = style.bg(bg_col);
        }
    } else if is_del {
        style = style.fg(theme.del_fg.into());
        if has_highlight {
            let bg_col = if is_selected {
                lerp_color(theme.cursor_bg.into(), del_bg, 0.6)
            } else {
                del_bg
            };
            style = style.bg(bg_col);
        }
    }

    if !is_staged && (is_add || is_del) {
        style = style.fg(theme.dim.into());
        if !is_selected {
            style = style.bg(theme.bg.into());
        }
    }

    style
}

pub fn to_tui_style(style: syntect::highlighting::Style) -> Style {
    Style::default().fg(Color::Rgb(
        style.foreground.r,
        style.foreground.g,
        style.foreground.b,
    ))
}

pub struct LineBgCalculator {
    grad1_width: f32,
    grad2_width: f32,
    use_grad_change: bool,
    use_gradient_change_solid: bool,
    use_staged_grad: bool,
    use_staged_solid: bool,
    change_bg: Color,
    end_bg: Color,
    accent_bg_grad: Color,
    is_staged: bool,
    is_add_or_del: bool,
}

impl LineBgCalculator {
    pub fn new(
        is_add: bool,
        is_del: bool,
        is_selected: bool,
        is_staged: bool,
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

        let is_add_or_del = is_add || is_del;
        let use_grad_change = use_gradient
            && is_add_or_del
            && matches!(theme.file_change_highlight, LineHighlightMode::Gradient(_));
        let use_gradient_change_solid = use_gradient
            && is_staged
            && theme.file_change_highlight == LineHighlightMode::Solid;

        let use_staged_grad = is_staged
            && is_add_or_del
            && matches!(theme.file_staged_highlight, LineHighlightMode::Gradient(_));
        let use_staged_solid = is_staged
            && is_add_or_del
            && theme.file_staged_highlight == LineHighlightMode::Solid;

        let end_bg: Color = if is_selected {
            theme.cursor_bg.into()
        } else {
            theme.bg.into()
        };
        let raw_change_bg: Color = if is_add {
            theme.add_bg.into()
        } else {
            theme.del_bg.into()
        };
        let change_bg = lerp_color(
            raw_change_bg,
            end_bg,
            1.0 - theme.file_change_highlight_opacity as f32,
        );
        let accent_bg_grad = lerp_color(
            theme.bg.into(),
            theme.partial.into(),
            theme.file_staged_highlight_opacity as f32,
        );

        Self {
            grad1_width,
            grad2_width,
            use_grad_change,
            use_gradient_change_solid,
            use_staged_grad,
            use_staged_solid,
            change_bg,
            end_bg,
            accent_bg_grad,
            is_staged,
            is_add_or_del,
        }
    }

    pub fn get_bg(&self, visual_x: usize) -> Color {
        let base_bg = if self.use_grad_change {
            let t1 = (visual_x as f32 / self.grad1_width).clamp(0.0, 1.0);
            lerp_color(self.change_bg, self.end_bg, t1)
        } else if self.use_gradient_change_solid {
            self.change_bg
        } else {
            self.end_bg
        };

        if self.is_staged && self.is_add_or_del {
            if self.use_staged_grad {
                let t2 = (visual_x as f32 / self.grad2_width).clamp(0.0, 1.0);
                lerp_color(self.accent_bg_grad, base_bg, t2)
            } else if self.use_staged_solid {
                self.accent_bg_grad
            } else {
                base_bg
            }
        } else {
            base_bg
        }
    }

    pub fn char_by_char(&self) -> bool {
        self.use_grad_change || self.use_staged_grad
    }
}
