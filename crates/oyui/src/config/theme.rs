use rune::Any;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Any)]
pub enum LineHighlightMode {
    #[rune(constructor)]
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

    #[builder(default = "◣".to_string())]
    pub char_hunk_split: String,
    #[builder(default = None)]
    pub char_hunk_split_color: Option<Color>,
    #[builder(default = "▶".to_string())]
    pub char_line_split: String,
    #[builder(default = None)]
    pub char_line_split_color: Option<Color>,
    #[builder(default = "▎".to_string())]
    pub char_indicator: String,
    #[builder(default = "+ ".to_string())]
    pub char_add_sign: String,
    #[builder(default = "- ".to_string())]
    pub char_del_sign: String,
}
