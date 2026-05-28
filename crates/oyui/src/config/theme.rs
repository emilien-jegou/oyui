use serde::{Deserialize, Serialize};
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

/// Parse a `#rrggbb` hex string into a [`Color`].
pub fn parse_hex_color(s: &str) -> Option<Color> {
    if s.starts_with('#') && s.len() >= 7 {
        let r = u8::from_str_radix(&s[1..3], 16).ok()?;
        let g = u8::from_str_radix(&s[3..5], 16).ok()?;
        let b = u8::from_str_radix(&s[5..7], 16).ok()?;
        Some(Color::Rgb(r, g, b))
    } else {
        None
    }
}

/// Parse a highlight mode string: `"none"`, `"solid"`, `"gradient"`,
/// or `"gradient:15%"` / `"gradient:0.15"`.
pub fn parse_highlight_mode(s: &str, default_percentage: f64) -> Option<LineHighlightMode> {
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
