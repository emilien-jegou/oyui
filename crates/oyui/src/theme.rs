//! Unified color and theme domain.

use crate::config::theme::Color;
use crate::config::UiTheme;
use crate::terminal_colors::TerminalColorMode;

/// The application's current UI + syntect theme, shared with workers.
pub struct ThemeState {
    /// Resolved terminal colors for widgets.
    pub ui: UiTheme,
    /// Highlighting theme for syntax rendering.
    pub tm_theme: Option<syntect::highlighting::Theme>,
}

impl ThemeState {
    /// Builds the ANSI-derived default theme for a color mode.
    pub fn new(color_mode: &TerminalColorMode) -> Self {
        Self {
            ui: ansi_default_theme(color_mode),
            tm_theme: None,
        }
    }
}

/// Resolves an ANSI color to its RGB value if the terminal is in TrueColor mode
/// and has the corresponding palette entry populated.
pub fn resolve_color_for_mode(color: Color, color_mode: &TerminalColorMode) -> Color {
    if let TerminalColorMode::TrueColor(palette) = color_mode {
        if color == Color::Fg {
            if let Some((r, g, b)) = palette.fg {
                return Color::Rgb(r, g, b);
            }
        }

        if color == Color::Bg {
            if let Some((r, g, b)) = palette.bg {
                return Color::Rgb(r, g, b);
            }
        }

        let index = match color {
            Color::Ansi(i) => Some(i as usize),
            Color::Ansi256(i) => Some(i as usize),
            Color::Black => Some(0),
            Color::Red => Some(1),
            Color::Green => Some(2),
            Color::Yellow => Some(3),
            Color::Blue => Some(4),
            Color::Magenta => Some(5),
            Color::Cyan => Some(6),
            Color::Gray => Some(7),
            Color::DarkGray => Some(8),
            Color::LightRed => Some(9),
            Color::LightGreen => Some(10),
            Color::LightYellow => Some(11),
            Color::LightBlue => Some(12),
            Color::LightMagenta => Some(13),
            Color::LightCyan => Some(14),
            Color::White => Some(15),
            _ => None,
        };

        if let Some(idx) = index {
            if idx < palette.ansi.len() {
                if let Some((r, g, b)) = palette.ansi[idx] {
                    return Color::Rgb(r, g, b);
                }
            }
        }
    }
    color
}

/// Computes the linear interpolation between a background color and a target color.
pub fn blend_colors(bg: (u8, u8, u8), target: (u8, u8, u8), factor: f32) -> Color {
    let blend_channel = |c1: u8, c2: u8| -> u8 {
        (c1 as f32 * (1.0 - factor) + c2 as f32 * factor).clamp(0.0, 255.0) as u8
    };
    Color::Rgb(
        blend_channel(bg.0, target.0),
        blend_channel(bg.1, target.1),
        blend_channel(bg.2, target.2),
    )
}

/// Builds a fully ANSI-based default theme, delegating base background and foreground
/// contrast mapping back to the terminal emulator's theme preferences.
pub fn ansi_default_theme(color_mode: &TerminalColorMode) -> crate::config::UiTheme {
    use crate::config::UiTheme;

    let bg = resolve_color_for_mode(Color::Bg, color_mode);
    let fg = resolve_color_for_mode(Color::Fg, color_mode);

    let (dim, dimmer, cursor_bg, subtle) = match color_mode {
        TerminalColorMode::TrueColor(palette) => {
            if let Some(bg_rgb) = palette.bg {
                let luminance =
                    0.299 * bg_rgb.0 as f32 + 0.587 * bg_rgb.1 as f32 + 0.114 * bg_rgb.2 as f32;
                let is_dark = luminance < 128.0;

                let fg_rgb =
                    palette
                        .fg
                        .unwrap_or_else(|| if is_dark { (255, 255, 255) } else { (0, 0, 0) });

                let cursor_target = if is_dark { (255, 255, 255) } else { (0, 0, 0) };

                (
                    blend_colors(bg_rgb, fg_rgb, 0.65),
                    blend_colors(bg_rgb, fg_rgb, 0.45),
                    blend_colors(bg_rgb, cursor_target, 0.15),
                    blend_colors(bg_rgb, fg_rgb, 0.30),
                )
            } else {
                default_fallbacks(color_mode)
            }
        }
        _ => default_fallbacks(color_mode),
    };

    UiTheme::builder()
        .bg(bg)
        .fg(fg)
        .dim(dim)
        .dimmer(dimmer)
        .cursor_bg(cursor_bg)
        .staged(resolve_color_for_mode(Color::Green, color_mode))
        .unstaged(dim)
        .partial(resolve_color_for_mode(Color::Yellow, color_mode))
        .dir(resolve_color_for_mode(Color::Blue, color_mode))
        .cmd(resolve_color_for_mode(Color::Magenta, color_mode))
        .add_bg(resolve_color_for_mode(Color::Green, color_mode))
        .add_fg(resolve_color_for_mode(Color::Green, color_mode))
        .del_bg(resolve_color_for_mode(Color::Red, color_mode))
        .del_fg(resolve_color_for_mode(Color::Red, color_mode))
        .char_trailing_space_fg(subtle)
        .char_tab_fg(subtle)
        .char_scroll_fg(subtle)
        .build()
}

/// Fallback colors used when true color mode or background details are unavailable.
fn default_fallbacks(color_mode: &TerminalColorMode) -> (Color, Color, Color, Color) {
    (
        resolve_color_for_mode(Color::Gray, color_mode),
        resolve_color_for_mode(Color::DarkGray, color_mode),
        resolve_color_for_mode(Color::DarkGray, color_mode),
        resolve_color_for_mode(Color::DarkGray, color_mode),
    )
}
