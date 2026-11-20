use crate::config::theme::Color;

pub mod define;
pub mod handlers;
pub mod keybinds;

pub use define::*;
pub use keybinds::*;

/// Parse a `#rrggbb` hex string into a [`Color`].
///
/// Exactly three hex pairs: anything after the sixth digit is rejected rather
/// than silently ignored (finding 6.18), so `#1234567` and non-hex text both
/// fail to parse instead of coloring arbitrarily.
pub fn parse_hex_color(s: &str) -> Option<Color> {
    let hex = s.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    // Byte-slicing at fixed offsets is safe only if every pair is ASCII hex;
    // `get` returns `None` instead of panicking on a multi-byte char boundary.
    let r = u8::from_str_radix(hex.get(0..2)?, 16).ok()?;
    let g = u8::from_str_radix(hex.get(2..4)?, 16).ok()?;
    let b = u8::from_str_radix(hex.get(4..6)?, 16).ok()?;
    Some(Color::Rgb(r, g, b))
}
