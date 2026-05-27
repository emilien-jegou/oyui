use ratatui::style::Color;

pub fn lerp_color(c1: Color, c2: Color, t: f32) -> Color {
    let (r1, g1, b1) = match c1 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };
    let (r2, g2, b2) = match c2 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    };
    Color::Rgb(
        (r1 as f32 + (r2 as f32 - r1 as f32) * t).clamp(0.0, 255.0) as u8,
        (g1 as f32 + (g2 as f32 - g1 as f32) * t).clamp(0.0, 255.0) as u8,
        (b1 as f32 + (b2 as f32 - b1 as f32) * t).clamp(0.0, 255.0) as u8,
    )
}

pub fn desaturate_color(c: Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return c,
    };
    let l = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    Color::Rgb(
        (r as f32 + (l - r as f32) * factor).clamp(0.0, 255.0) as u8,
        (g as f32 + (l - g as f32) * factor).clamp(0.0, 255.0) as u8,
        (b as f32 + (l - b as f32) * factor).clamp(0.0, 255.0) as u8,
    )
}

pub fn darken_color(c: Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return c,
    };
    let mult = (1.0 - factor).clamp(0.0, 1.0);
    Color::Rgb(
        (r as f32 * mult) as u8,
        (g as f32 * mult) as u8,
        (b as f32 * mult) as u8,
    )
}

pub fn lighten_color(c: Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => return c,
    };
    Color::Rgb(
        (r as f32 + (255.0 - r as f32) * factor).clamp(0.0, 255.0) as u8,
        (g as f32 + (255.0 - g as f32) * factor).clamp(0.0, 255.0) as u8,
        (b as f32 + (255.0 - b as f32) * factor).clamp(0.0, 255.0) as u8,
    )
}

pub fn is_dark(bg: Color) -> bool {
    let (r, g, b) = match bg {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    luminance < 128.0
}
