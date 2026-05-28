use std::collections::HashMap;
use std::env;
use std::path::Path;
use syntect::highlighting::{Theme, ThemeSet};

#[path = "src/config/theme.rs"]
mod theme;

#[path = "src/config/define_default_theme.rs"]
mod define_default_theme;

use define_default_theme::derive_ui_theme;
use theme::UiTheme;

fn main() {
    println!("cargo:rerun-if-changed=themes");
    println!("cargo:rerun-if-changed=src/config/theme.rs");
    println!("cargo:rerun-if-changed=src/config/define_default_theme.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("themes.bin");

    let mut ts = ThemeSet::new();

    if Path::new("themes").exists() {
        ts.add_from_folder("themes").expect("Failed to load tmThemes");
    }

    // Derive a UiTheme for every .tmTheme and embed the pair.
    // Runtime overrides are now applied by the Rune script, not at compile time.
    let embedded_themes: HashMap<String, (UiTheme, Theme)> = ts
        .themes
        .into_iter()
        .map(|(name, tm_theme)| {
            let ui_theme = derive_ui_theme(&tm_theme);
            (name, (ui_theme, tm_theme))
        })
        .collect();

    syntect::dumps::dump_to_file(&embedded_themes, dest_path)
        .expect("Failed to dump embedded themes");
}
