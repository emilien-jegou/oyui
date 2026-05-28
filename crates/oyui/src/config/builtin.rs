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

pub fn fallback_theme(base_theme_name: &str) -> (UiTheme, Theme) {
    let themes = get_embedded_themes();
    themes
        .get(base_theme_name)
        .or_else(|| themes.values().next())
        .expect("At least one theme should be present")
        .clone()
}
