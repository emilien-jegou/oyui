use super::theme::UiTheme;
use std::collections::HashMap;
use std::sync::OnceLock;
use syntect::highlighting::Theme;

pub type EmbeddedThemeMap = HashMap<String, (UiTheme, Theme)>;

static EMBEDDED_THEMES: OnceLock<EmbeddedThemeMap> = OnceLock::new();

pub fn get_embedded_themes() -> &'static EmbeddedThemeMap {
    EMBEDDED_THEMES.get_or_init(|| {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/themes.bin"));
        syntect::dumps::from_binary(bytes)
    })
}

pub fn get_theme(theme_name: &str) -> Option<(UiTheme, Option<Theme>)> {
    let themes = get_embedded_themes();
    let pair = themes.get(theme_name).cloned().map(|s| (s.0, Some(s.1)));
    pair
}
