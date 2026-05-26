use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use syntect::highlighting::{Theme, ThemeSet};

// Load the source files directly into build.rs at the root level.
// This prevents Rust from looking inside a phantom `config/` directory.
#[path = "src/config/theme.rs"]
mod theme;

#[path = "src/config/define_default_theme.rs"]
mod define_default_theme;

use define_default_theme::derive_ui_theme;
use theme::{ThemeConfig, UiTheme};

fn main() {
    println!("cargo:rerun-if-changed=themes");
    println!("cargo:rerun-if-changed=src/config/theme.rs");
    println!("cargo:rerun-if-changed=src/config/define_default_theme.rs");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("themes.bin");

    let mut ts = ThemeSet::new();

    // 1. Safely load all .tmTheme files from the directory
    if Path::new("themes").exists() {
        ts.add_from_folder("themes").expect("Failed to load tmThemes");
    }

    // 2. Derive base themes and apply TOML overrides
    let mut embedded_themes: HashMap<String, (UiTheme, Theme)> = HashMap::new();

    for (name, tm_theme) in ts.themes {
        let mut ui_theme = derive_ui_theme(&tm_theme);

        let toml_path = Path::new("themes").join(format!("{}.toml", name));
        if toml_path.exists() {
            let toml_str = fs::read_to_string(toml_path).expect("Failed to read TOML override");
            if let Ok(config) = toml::from_str::<ThemeConfig>(&toml_str) {
                // Apply embedded TOML overrides at compile time!
                ui_theme.apply_overrides(&config.colors, Some(&tm_theme));
            }
        }

        embedded_themes.insert(name, (ui_theme, tm_theme));
    }

    // 3. Dump the fully pre-computed HashMap directly to binary
    syntect::dumps::dump_to_file(&embedded_themes, dest_path)
        .expect("Failed to dump embedded themes");
}
