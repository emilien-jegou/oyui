use crate::config::UiTheme;
use crate::{
    actions::handlers::theme_handler::utils::resolve_color_for_mode,
    terminal_colors::TerminalColorMode,
};
use crate::{app::CommandMode, config::theme::Color};
use parking_lot::RwLock;
use std::sync::atomic::AtomicBool;

/// Builds a fully ANSI-based default theme, delegating base background and foreground
/// contrast mapping back to the terminal emulator's theme preferences.
pub fn ansi_default_theme(color_mode: &TerminalColorMode) -> UiTheme {
    let bg = resolve_color_for_mode(Color::Bg, color_mode);
    let fg = resolve_color_for_mode(Color::Fg, color_mode);

    // Derive our dynamic text/structural colors based on palette characteristics
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
                    blend_colors(bg_rgb, fg_rgb, 0.65),        // dim
                    blend_colors(bg_rgb, fg_rgb, 0.45),        // dimmer
                    blend_colors(bg_rgb, cursor_target, 0.15), // cursor_bg
                    blend_colors(bg_rgb, fg_rgb, 0.30),        // subtle (whitespace/structural)
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

/// Computes the linear interpolation between a background color and a target color.
fn blend_colors(bg: (u8, u8, u8), target: (u8, u8, u8), factor: f32) -> Color {
    let blend_channel = |c1: u8, c2: u8| -> u8 {
        (c1 as f32 * (1.0 - factor) + c2 as f32 * factor).clamp(0.0, 255.0) as u8
    };
    Color::Rgb(
        blend_channel(bg.0, target.0),
        blend_channel(bg.1, target.1),
        blend_channel(bg.2, target.2),
    )
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

pub struct ThemeState {
    pub theme_name: String,
    pub ui: UiTheme,
    pub tm_theme: Option<syntect::highlighting::Theme>,
}

impl ThemeState {
    pub fn new(color_mode: &TerminalColorMode) -> Self {
        let ui = ansi_default_theme(color_mode);

        Self {
            theme_name: "ansi".into(),
            ui,
            tm_theme: None,
        }
    }
}

pub struct TuiState {
    pub theme: RwLock<ThemeState>,
    pub should_quit: AtomicBool,
    pub command_mode: RwLock<CommandMode>,
    pub confirm_merge_window_enabled: AtomicBool,
}

impl TuiState {
    pub fn new(color_mode: &TerminalColorMode) -> Self {
        Self {
            theme: RwLock::new(ThemeState::new(color_mode)),
            should_quit: AtomicBool::new(false),
            command_mode: RwLock::new(CommandMode::Normal),
            confirm_merge_window_enabled: AtomicBool::new(false),
        }
    }
}
