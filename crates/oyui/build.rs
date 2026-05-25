use std::env;
use std::path::Path;
use syntect::highlighting::ThemeSet;

fn main() {
    // Tell Cargo to recompile if themes are added/modified
    println!("cargo:rerun-if-changed=themes");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("themes.bin");

    let mut ts = ThemeSet::new();

    // Safely load all .tmTheme files from the directory
    if Path::new("themes").exists() {
        ts.add_from_folder("themes")
            .expect("Failed to load tmThemes");
    }

    // Dump the entire parsed ThemeSet to a binary file using bincode
    syntect::dumps::dump_to_file(&ts, dest_path).expect("Failed to dump ThemeSet");
}
