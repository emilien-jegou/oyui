use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use syntect::highlighting::Theme;
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LineHighlightMode {
    None,
    Solid,
    Gradient(f64),
}

impl PartialEq for LineHighlightMode {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Solid, Self::Solid) => true,
            (Self::Gradient(a), Self::Gradient(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}
impl Eq for LineHighlightMode {}

#[derive(TypedBuilder, Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    #[builder(default = LineHighlightMode::Gradient(0.08))]
    pub file_staged_highlight: LineHighlightMode,
    #[builder(default = 0.3)]
    pub file_staged_highlight_opacity: f64,

    #[builder(default = LineHighlightMode::Gradient(0.2))]
    pub file_change_highlight: LineHighlightMode,
    #[builder(default = 0.1)]
    pub file_change_highlight_opacity: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ThemeConfig {
    #[allow(dead_code)]
    pub tm_theme_path: Option<String>,

    // Flatten and use toml::Value to handle mixed types (strings, integers, floats)
    #[serde(flatten, default)]
    pub colors: HashMap<String, toml::Value>,
}

fn parse_highlight_mode(s: &str, default_percentage: f64) -> Option<LineHighlightMode> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "none" => Some(LineHighlightMode::None),
        "solid" => Some(LineHighlightMode::Solid),
        "gradient" => Some(LineHighlightMode::Gradient(default_percentage)),
        _ if s.starts_with("gradient:") => {
            let val_str = s.strip_prefix("gradient:").unwrap().trim();
            let percentage = if let Some(stripped) = val_str.strip_suffix('%') {
                stripped.parse::<f64>().ok().map(|p| p / 100.0)
            } else {
                val_str.parse::<f64>().ok()
            };
            percentage.map(LineHighlightMode::Gradient)
        }
        _ => None,
    }
}

impl UiTheme {
    pub fn apply_overrides(
        &mut self,
        config: &HashMap<String, toml::Value>,
        tm_theme: Option<&Theme>,
    ) {
        let get_col = |key: &str, default: Color| -> Color {
            resolve_color(config.get(key), tm_theme, default)
        };
        let get_mode =
            |key: &str, default: LineHighlightMode, default_pct: f64| -> LineHighlightMode {
                config
                    .get(key)
                    .and_then(|v| v.as_str()) // Extract string value if it is a string
                    .and_then(|s| parse_highlight_mode(s, default_pct))
                    .unwrap_or(default)
            };
        let get_f64 = |key: &str, default: f64| -> f64 {
            config
                .get(key)
                .and_then(|v| {
                    // Try parsing as float first, then fallback to converting an integer
                    v.as_float()
                        .or_else(|| v.as_integer().map(|i| i as f64))
                        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
                })
                .unwrap_or(default)
        };

        self.bg = get_col("bg", self.bg);
        self.cursor_bg = get_col("cursor_bg", self.cursor_bg);
        self.fg = get_col("fg", self.fg);
        self.dim = get_col("dim", self.dim);
        self.staged = get_col("staged", self.staged);
        self.unstaged = get_col("unstaged", self.unstaged);
        self.partial = get_col("partial", self.partial);
        self.dir = get_col("dir", self.dir);
        self.cmd = get_col("cmd", self.cmd);
        self.add_bg = get_col("add_bg", self.add_bg);
        self.del_bg = get_col("del_bg", self.del_bg);
        self.add_fg = get_col("add_fg", self.add_fg);
        self.del_fg = get_col("del_fg", self.del_fg);

        self.file_staged_highlight =
            get_mode("file_staged_highlight", self.file_staged_highlight, 0.08);
        self.file_staged_highlight_opacity = get_f64(
            "file_staged_highlight_opacity",
            self.file_staged_highlight_opacity,
        );
        self.file_change_highlight =
            get_mode("file_change_highlight", self.file_change_highlight, 0.50);
        self.file_change_highlight_opacity = get_f64(
            "file_change_highlight_opacity",
            self.file_change_highlight_opacity,
        );
    }
}

pub fn resolve_color(val: Option<&toml::Value>, tm_theme: Option<&Theme>, default: Color) -> Color {
    let Some(v) = val else { return default };
    let Some(s) = v.as_str() else { return default }; // Color values must be strings

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
