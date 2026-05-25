use super::theme::UiTheme;
use ratatui::style::Color;
use std::sync::OnceLock;
use syntect::highlighting::{Theme, ThemeSet};

static EMBEDDED_THEMES: OnceLock<ThemeSet> = OnceLock::new();

/// Loads the pre-compiled binary ThemeSet dumped by build.rs
pub fn get_embedded_themes() -> &'static ThemeSet {
    EMBEDDED_THEMES.get_or_init(|| {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/themes.bin"));
        syntect::dumps::from_binary(bytes)
    })
}

/// Core function that extracts TUI colors intelligently from ANY .tmTheme
pub fn derive_ui_theme(theme: &Theme) -> UiTheme {
    let bg = syn_to_color(theme.settings.background).unwrap_or(Color::Rgb(14, 14, 18));
    let fg = syn_to_color(theme.settings.foreground).unwrap_or(Color::Rgb(200, 200, 210));

    let cursor_bg = syn_to_color(theme.settings.line_highlight)
        .or_else(|| syn_to_color(theme.settings.selection))
        .unwrap_or_else(|| blend(fg, bg, 0.1)); // Fallback: slight highlight

    let dim = extract_scope_color(theme, &["comment", "punctuation"])
        .or_else(|| syn_to_color(theme.settings.gutter_foreground))
        .unwrap_or(Color::Rgb(90, 90, 105));

    // Derive diff / git colors from semantic syntax highlighting
    let staged = extract_scope_color(theme, &["markup.inserted", "string", "entity.name.string"])
        .unwrap_or(Color::Rgb(130, 210, 150));

    let partial = extract_scope_color(
        theme,
        &["markup.changed", "constant.numeric", "support.type"],
    )
    .unwrap_or(Color::Rgb(210, 170, 80));

    let del_fg = extract_scope_color(theme, &["markup.deleted", "invalid", "keyword.operator"])
        .unwrap_or(Color::Rgb(255, 130, 130));

    let dir = extract_scope_color(theme, &["entity.name.type", "entity.name.class", "storage"])
        .unwrap_or(Color::Rgb(100, 140, 210));

    let cmd = extract_scope_color(
        theme,
        &["keyword.control", "variable", "entity.name.function"],
    )
    .unwrap_or(Color::Rgb(180, 140, 255));

    UiTheme {
        bg,
        cursor_bg,
        fg,
        dim,
        staged,
        unstaged: dim,
        partial,
        dir,
        cmd,
        add_bg: blend(staged, bg, 0.15),
        del_bg: blend(del_fg, bg, 0.15),
        add_fg: staged,
        del_fg,
    }
}

pub fn fallback_theme() -> (UiTheme, Theme) {
    // If absolutely nothing is provided, construct a baseline dark theme.
    // (In reality, if "catppuccin-mocha.tmTheme" is embedded, it will use that)
    let themes = get_embedded_themes();
    let theme = themes
        .themes
        .get("catppuccin-mocha")
        .or_else(|| themes.themes.values().next())
        .unwrap()
        .clone();

    (derive_ui_theme(&theme), theme)
}

fn syn_to_color(opt: Option<syntect::highlighting::Color>) -> Option<Color> {
    opt.map(|c| Color::Rgb(c.r, c.g, c.b))
}

fn extract_scope_color(theme: &Theme, target_scopes: &[&str]) -> Option<Color> {
    for &target in target_scopes {
        for item in &theme.scopes {
            // Using Debug format string allows us to safely substring search the AST
            if format!("{:?}", item.scope).contains(target) {
                if let Some(c) = item.style.foreground {
                    return Some(Color::Rgb(c.r, c.g, c.b));
                }
            }
        }
    }
    None
}

fn blend(fg: Color, bg: Color, alpha: f32) -> Color {
    let (fr, fg_g, fb) = match fg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    };
    let (br, bg_g, bb) = match bg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };

    Color::Rgb(
        ((fr as f32 * alpha) + (br as f32 * (1.0 - alpha))) as u8,
        ((fg_g as f32 * alpha) + (bg_g as f32 * (1.0 - alpha))) as u8,
        ((fb as f32 * alpha) + (bb as f32 * (1.0 - alpha))) as u8,
    )
}
