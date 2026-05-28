use crate::config::theme::Color;

pub mod define;
pub mod handlers;
pub mod keybinds;
pub mod state;

pub use define::*;
pub use keybinds::*;

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
