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

fn is_dark(bg: Color) -> bool {
    match bg {
        Color::Rgb(r, g, b) => {
            // Standard perceived luminance formula
            let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            luminance < 128.0
        }
        _ => true, // assume dark if non-RGB
    }
}

/// Core function that extracts TUI colors intelligently from ANY .tmTheme
pub fn derive_ui_theme(theme: &Theme) -> UiTheme {
    // background is required — panic with a clear message if absent
    let bg = syn_to_color(theme.settings.background)
        .expect("tmTheme is missing a background color, which is required");

    let dark = is_dark(bg);

    let fg = syn_to_color(theme.settings.foreground).unwrap_or_else(|| {
        let default = if dark {
            Color::Rgb(200, 200, 210)
        } else {
            Color::Rgb(40, 40, 50)
        };
        eprintln!("warning: tmTheme missing 'foreground', using default {:?}", default);
        default
    });

    let cursor_bg = syn_to_color(theme.settings.line_highlight).unwrap_or_else(|| {
        let derived = blend(fg, bg, 0.08);
        eprintln!(
            "warning: tmTheme missing 'lineHighlight', deriving cursor_bg from bg as {:?}",
            derived
        );
        derived
    });

    let dim = extract_scope_color(theme, &["comment", "punctuation"])
        .or_else(|| syn_to_color(theme.settings.gutter_foreground))
        .unwrap_or_else(|| {
            let default = if dark {
                Color::Rgb(90, 90, 105)
            } else {
                Color::Rgb(150, 150, 160)
            };
            eprintln!("warning: tmTheme missing comment/gutter color for 'dim', using default {:?}", default);
            default
        });

    let staged = extract_scope_color(theme, &["markup.inserted", "string", "entity.name.string"])
        .unwrap_or_else(|| {
            let default = if dark {
                Color::Rgb(130, 210, 150)
            } else {
                Color::Rgb(40, 140, 70)
            };
            eprintln!("warning: tmTheme missing inserted/string color for 'staged', using default {:?}", default);
            default
        });

    let partial = extract_scope_color(
        theme,
        &["markup.changed", "constant.numeric", "support.type"],
    )
    .unwrap_or_else(|| {
        let default = if dark {
            Color::Rgb(210, 170, 80)
        } else {
            Color::Rgb(160, 110, 20)
        };
        eprintln!("warning: tmTheme missing changed/numeric color for 'partial', using default {:?}", default);
        default
    });

    let del_fg = extract_scope_color(theme, &["markup.deleted", "invalid", "keyword.operator"])
        .unwrap_or_else(|| {
            let default = if dark {
                Color::Rgb(255, 130, 130)
            } else {
                Color::Rgb(180, 40, 40)
            };
            eprintln!("warning: tmTheme missing deleted/invalid color for 'del_fg', using default {:?}", default);
            default
        });

    let dir = extract_scope_color(theme, &["entity.name.type", "entity.name.class", "storage"])
        .unwrap_or_else(|| {
            let default = if dark {
                Color::Rgb(100, 140, 210)
            } else {
                Color::Rgb(30, 80, 170)
            };
            eprintln!("warning: tmTheme missing type/class color for 'dir', using default {:?}", default);
            default
        });

    let cmd = extract_scope_color(
        theme,
        &["keyword.control", "variable", "entity.name.function"],
    )
    .unwrap_or_else(|| {
        let default = if dark {
            Color::Rgb(180, 140, 255)
        } else {
            Color::Rgb(100, 60, 180)
        };
        eprintln!("warning: tmTheme missing keyword/function color for 'cmd', using default {:?}", default);
        default
    });

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
    let themes = get_embedded_themes();
    let theme = themes
        .themes
        .get("quaoar")
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
