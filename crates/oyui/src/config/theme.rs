use ratatui::style::Color;
use serde::Deserialize;
use std::collections::HashMap;
use syntect::highlighting::Theme;

#[derive(Debug, Deserialize, Clone)]
pub struct ThemeConfig {
    pub tm_theme_path: Option<String>,
    #[serde(default)]
    pub colors: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiTheme {
    pub bg: Color,
    pub cursor_bg: Color,
    pub fg: Color,
    pub dim: Color,
    pub staged: Color,
    pub unstaged: Color,
    pub partial: Color,
    pub dir: Color,
    pub cmd: Color,
    pub add_bg: Color,
    pub del_bg: Color,
    pub add_fg: Color,
    pub del_fg: Color,
}

pub fn resolve_color(val: Option<&String>, tm_theme: Option<&Theme>, default: Color) -> Color {
    let Some(s) = val else { return default };

    if let Some(name) = s.strip_prefix("tm:") {
        if let Some(t) = tm_theme {
            let syn_col = match name {
                "foreground" => t.settings.foreground,
                "background" => t.settings.background,
                "caret" => t.settings.caret,
                "line_highlight" => t.settings.line_highlight,
                "misspelling" => t.settings.misspelling,
                "minimap_border" => t.settings.minimap_border,
                "accent" => t.settings.accent,
                "bracket_contents_foreground" => t.settings.bracket_contents_foreground,
                "brackets_foreground" => t.settings.brackets_foreground,
                "brackets_background" => t.settings.brackets_background,
                "tags_foreground" => t.settings.tags_foreground,
                "highlight" => t.settings.highlight,
                "find_highlight" => t.settings.find_highlight,
                "find_highlight_foreground" => t.settings.find_highlight_foreground,
                "gutter" => t.settings.gutter,
                "gutter_foreground" => t.settings.gutter_foreground,
                "selection" => t.settings.selection,
                "selection_foreground" => t.settings.selection_foreground,
                "selection_border" => t.settings.selection_border,
                "inactive_selection" => t.settings.inactive_selection,
                "inactive_selection_foreground" => t.settings.inactive_selection_foreground,
                "guide" => t.settings.guide,
                "active_guide" => t.settings.active_guide,
                "stack_guide" => t.settings.stack_guide,
                "shadow" => t.settings.shadow,
                _ => None,
            };
            if let Some(c) = syn_col {
                return Color::Rgb(c.r, c.g, c.b);
            }
        }
        return default;
    }

    if s.starts_with('#') && s.len() >= 7 {
        let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0);
        return Color::Rgb(r, g, b);
    }

    default
}
