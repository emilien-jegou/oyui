use crate::actions::parse_hex_color;
use crate::config::theme::{Color, UiTheme};
use crate::terminal_colors::TerminalColorMode;
use syntect::highlighting::Theme as SynTheme;

/// Helper to determine if a string is a hex color code without the '#' prefix.
fn is_hex_string(s: &str) -> bool {
    let len = s.len();
    (len == 3 || len == 6) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Helper to parse "rgb(r, g, b)" format.
fn parse_rgb_parentheses(val: &str) -> Option<Color> {
    let rgb_stripped = val.strip_prefix("rgb(")?.strip_suffix(')')?;
    let mut parts = rgb_stripped.split(',');
    let r = parts.next()?.trim().parse::<u8>().ok()?;
    let g = parts.next()?.trim().parse::<u8>().ok()?;
    let b = parts.next()?.trim().parse::<u8>().ok()?;
    Some(Color::Rgb(r, g, b))
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

pub fn parse_color_val(
    val: &str,
    ui: &UiTheme,
    tm: &Option<SynTheme>,
    color_mode: &TerminalColorMode,
) -> Option<Color> {
    if color_mode == &TerminalColorMode::NoColor {
        return None;
    }

    let color = if let Some(theme_field) = val.strip_prefix("theme:") {
        match theme_field {
            "bg" => Some(ui.bg),
            "fg" => Some(ui.fg),
            "cursor_bg" => Some(ui.cursor_bg),
            "dim" => Some(ui.dim),
            "dimmer" => Some(ui.dimmer),
            "staged" => Some(ui.staged),
            "unstaged" => Some(ui.unstaged),
            "partial" => Some(ui.partial),
            "dir" => Some(ui.dir),
            "cmd" => Some(ui.cmd),
            "add_bg" => Some(ui.add_bg),
            "del_bg" => Some(ui.del_bg),
            "add_fg" => Some(ui.add_fg),
            "del_fg" => Some(ui.del_fg),
            "char_trailing_space_fg" => Some(ui.char_trailing_space_fg),
            "char_tab_fg" => Some(ui.char_tab_fg),
            "char_scroll_fg" => Some(ui.char_scroll_fg),
            "char_line_split_color" => ui.char_line_split_color,
            "char_hunk_split_color" => ui.char_hunk_split_color,
            _ => None,
        }
    } else if let Some(tm_field) = val.strip_prefix("tm:") {
        if let Some(ref inner_tm) = tm {
            let settings = &inner_tm.settings;
            let syn_color = match tm_field {
                "foreground" => settings.foreground,
                "background" => settings.background,
                "caret" => settings.caret,
                "line_highlight" => settings.line_highlight,
                "misspelling" => settings.misspelling,
                "minimap_border" => settings.minimap_border,
                "accent" => settings.accent,
                "bracket_contents_foreground" => settings.bracket_contents_foreground,
                "brackets_foreground" => settings.brackets_foreground,
                "brackets_background" => settings.brackets_background,
                "tags_foreground" => settings.tags_foreground,
                "highlight" => settings.highlight,
                "find_highlight" => settings.find_highlight,
                "find_highlight_foreground" => settings.find_highlight_foreground,
                "gutter" => settings.gutter,
                "gutter_foreground" => settings.gutter_foreground,
                "selection" => settings.selection,
                "selection_foreground" => settings.selection_foreground,
                "selection_border" => settings.selection_border,
                "inactive_selection" => settings.inactive_selection,
                "inactive_selection_foreground" => settings.inactive_selection_foreground,
                "guide" => settings.guide,
                "active_guide" => settings.active_guide,
                "stack_guide" => settings.stack_guide,
                "shadow" => settings.shadow,
                _ => None,
            };
            if let Some(c) = syn_color {
                Some(Color::Rgb(c.r, c.g, c.b))
            } else {
                crate::config::define_default_theme::extract_scope_color(inner_tm, &[tm_field])
            }
        } else {
            None
        }
    } else if let Some(ansi_val) = val.strip_prefix("ansi:") {
        ansi_val.parse::<u8>().ok().map(Color::Ansi)
    } else if let Some(ansi_val) = val.strip_prefix("ansi256:") {
        ansi_val.parse::<u8>().ok().map(Color::Ansi256)
    } else if val.starts_with('#') {
        parse_hex_color(val)
    } else if is_hex_string(val) {
        parse_hex_color(&format!("#{}", val))
    } else if val.starts_with("rgb(") {
        parse_rgb_parentheses(val)
    } else {
        match val.to_lowercase().as_str() {
            "reset" | "default" => Some(Color::Reset),
            "bg" => Some(Color::Bg),
            "fg" => Some(Color::Fg),
            "black" => Some(Color::Black),
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "yellow" => Some(Color::Yellow),
            "blue" => Some(Color::Blue),
            "magenta" => Some(Color::Magenta),
            "cyan" => Some(Color::Cyan),
            "gray" | "grey" => Some(Color::Gray),
            "darkgray" | "darkgrey" => Some(Color::DarkGray),
            "lightred" => Some(Color::LightRed),
            "lightgreen" => Some(Color::LightGreen),
            "lightyellow" => Some(Color::LightYellow),
            "lightblue" => Some(Color::LightBlue),
            "lightmagenta" => Some(Color::LightMagenta),
            "lightcyan" => Some(Color::LightCyan),
            "white" => Some(Color::White),
            _ => None,
        }
    };

    let resolved = color.map(|c| resolve_color_for_mode(c, color_mode));

    resolved.and_then(|c| match color_mode {
        TerminalColorMode::Ansi => match c {
            Color::Rgb(..) | Color::Ansi256(_) => None,
            _ => Some(c),
        },
        TerminalColorMode::Ansi256 => match c {
            Color::Rgb(..) => None,
            _ => Some(c),
        },
        _ => Some(c),
    })
}
