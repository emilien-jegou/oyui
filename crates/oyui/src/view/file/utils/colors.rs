use crate::config::theme::{Color, ColorRgb};

pub fn try_lerp_color(c1: &Color, c2: &Color, t: f32) -> Option<Color> {
    let rgb1: ColorRgb = (*c1).try_into().ok()?;
    let rgb2: ColorRgb = (*c2).try_into().ok()?;
    Some(lerp_color(&rgb1, &rgb2, t))
}

pub fn safe_lerp_color(c1: &Color, c2: &Color, t: f32) -> Color {
    try_lerp_color(c1, c2, t).unwrap_or(if t < 0.5 { *c1 } else { *c2 })
}

pub fn lerp_color(c1: &ColorRgb, c2: &ColorRgb, t: f32) -> Color {
    let ColorRgb(r1, g1, b1) = c1;
    let ColorRgb(r2, g2, b2) = c2;

    // Helper closure to keep the math readable
    let lerp = |a: u8, b: u8| -> u8 {
        let a = a as f32;
        (a + (b as f32 - a) * t).clamp(0.0, 255.0) as u8
    };

    Color::Rgb(lerp(*r1, *r2), lerp(*g1, *g2), lerp(*b1, *b2))
}

pub fn desaturate_color(c: &Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (*r, *g, *b),
        _ => return c.clone(),
    };
    let l = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    Color::Rgb(
        (r as f32 + (l - r as f32) * factor).clamp(0.0, 255.0) as u8,
        (g as f32 + (l - g as f32) * factor).clamp(0.0, 255.0) as u8,
        (b as f32 + (l - b as f32) * factor).clamp(0.0, 255.0) as u8,
    )
}

pub fn darken_color(c: &Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (*r, *g, *b),
        _ => return c.clone(),
    };
    let mult = (1.0 - factor).clamp(0.0, 1.0);
    Color::Rgb(
        (r as f32 * mult) as u8,
        (g as f32 * mult) as u8,
        (b as f32 * mult) as u8,
    )
}

pub fn lighten_color(c: &Color, factor: f32) -> Color {
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (*r, *g, *b),
        _ => return c.clone(),
    };
    Color::Rgb(
        (r as f32 + (255.0 - r as f32) * factor).clamp(0.0, 255.0) as u8,
        (g as f32 + (255.0 - g as f32) * factor).clamp(0.0, 255.0) as u8,
        (b as f32 + (255.0 - b as f32) * factor).clamp(0.0, 255.0) as u8,
    )
}
