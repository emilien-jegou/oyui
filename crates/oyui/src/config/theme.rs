use rune::Any;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Rgb(u8, u8, u8),
    Ansi(u8),
    Ansi256(u8),
    Reset,
    Bg,
    Fg,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

impl Color {
    #[allow(unused)]
    pub fn to_string_val(&self) -> String {
        match self {
            Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            Color::Ansi(v) => format!("ansi:{}", v),
            Color::Ansi256(v) => format!("ansi256:{}", v),
            Color::Reset => "reset".to_string(),
            Color::Bg => "bg".to_string(),
            Color::Fg => "fg".to_string(),
            Color::Black => "black".to_string(),
            Color::Red => "red".to_string(),
            Color::Green => "green".to_string(),
            Color::Yellow => "yellow".to_string(),
            Color::Blue => "blue".to_string(),
            Color::Magenta => "magenta".to_string(),
            Color::Cyan => "cyan".to_string(),
            Color::Gray => "gray".to_string(),
            Color::DarkGray => "darkgray".to_string(),
            Color::LightRed => "lightred".to_string(),
            Color::LightGreen => "lightgreen".to_string(),
            Color::LightYellow => "lightyellow".to_string(),
            Color::LightBlue => "lightblue".to_string(),
            Color::LightMagenta => "lightmagenta".to_string(),
            Color::LightCyan => "lightcyan".to_string(),
            Color::White => "white".to_string(),
        }
    }

    pub fn try_as_rgb(&self) -> Option<(u8, u8, u8)> {
        match self {
            Color::Rgb(r, g, b) => Some((*r, *g, *b)),
            _ => None,
        }
    }
}

#[allow(unused)] // due to build.rs
pub struct ColorRgb(pub u8, pub u8, pub u8);

impl TryFrom<Color> for ColorRgb {
    type Error = eyre::Report;

    fn try_from(value: Color) -> Result<Self, Self::Error> {
        match value {
            Color::Rgb(r, g, b) => Ok(Self(r, g, b)),
            _ => eyre::bail!("color is not rgb"),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize, Any)]
pub enum LineHighlightMode {
    #[rune(constructor)]
    #[default]
    None,
    #[rune(constructor)]
    Solid,
    #[rune(constructor)]
    Gradient(#[rune(get)] f64),
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
    pub dimmer: Color,
    pub staged: Color,
    pub unstaged: Color,
    pub partial: Color,
    pub dir: Color,
    pub cmd: Color,
    pub add_bg: Color,
    pub del_bg: Color,
    pub add_fg: Color,
    pub del_fg: Color,
    pub char_trailing_space_fg: Color,
    pub char_tab_fg: Color,
    pub char_scroll_fg: Color,
    #[builder(default = None)]
    pub char_hunk_split_color: Option<Color>,
    #[builder(default = None)]
    pub char_line_split_color: Option<Color>,

    #[builder(default = LineHighlightMode::Gradient(0.15))]
    pub file_staged_highlight: LineHighlightMode,
    #[builder(default = 0.1)]
    pub file_staged_highlight_opacity: f64,

    #[builder(default = LineHighlightMode::Gradient(3.0))]
    pub file_change_highlight: LineHighlightMode,
    #[builder(default = 0.1)]
    pub file_change_highlight_opacity: f64,

    #[builder(default = "◣".to_string())]
    pub char_hunk_split: String,
    #[builder(default = "▶".to_string())]
    pub char_line_split: String,
    #[builder(default = "▎".to_string())]
    pub char_indicator: String,
    #[builder(default = "+ ".to_string())]
    pub char_add_sign: String,
    #[builder(default = "- ".to_string())]
    pub char_del_sign: String,

    #[builder(default = "•".to_string())]
    pub char_trailing_space: String,

    #[builder(default = "·   ".to_string())]
    pub char_tab: String,

    #[builder(default = "↔".to_string())]
    pub char_scroll_both: String,
    #[builder(default = "˂".to_string())]
    pub char_scroll_left: String,
    #[builder(default = "˃".to_string())]
    pub char_scroll_right: String,

    /// Dim small addition/deletions colors in tree view
    #[builder(default = false)]
    pub tree_progressive_change_dim: bool,
}
