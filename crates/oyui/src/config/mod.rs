use serde::Deserialize;
use std::collections::HashMap;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use syntect::highlighting::ThemeSet;

pub mod builtin;
pub mod theme;

pub use builtin::{derive_ui_theme, fallback_theme, get_embedded_themes};
pub use theme::{ThemeConfig, UiTheme};

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub chosen_theme: Option<String>,
    #[serde(default)]
    pub theme: HashMap<String, ThemeConfig>,
}

pub fn load_config_and_theme(
    path: &Path,
) -> Result<(UiTheme, syntect::highlighting::Theme), Box<dyn std::error::Error>> {
    let config_str = std::fs::read_to_string(path).unwrap_or_default();

    let app_config: AppConfig = toml::from_str(&config_str).unwrap_or_else(|_| AppConfig {
        chosen_theme: Some("catppuccin-mocha".to_string()),
        theme: HashMap::new(),
    });

    let chosen_name = app_config
        .chosen_theme
        .as_deref()
        .unwrap_or("catppuccin-mocha");

    let mut tm_theme = None;
    let (mut final_theme, mut fallback_tm) = fallback_theme();
    let theme_cfg = app_config.theme.get(chosen_name);

    // 1. If user supplied a custom path, load it at runtime
    if let Some(path) = theme_cfg.and_then(|c| c.tm_theme_path.as_ref()) {
        if let Ok(file) = std::fs::File::open(path) {
            let mut reader = BufReader::new(file);
            if let Ok(ts) = ThemeSet::load_from_reader(&mut reader) {
                tm_theme = Some(ts);
            }
        }
    }
    // 2. Otherwise search the fast binary embedded set
    else if let Some(embedded_theme) = get_embedded_themes().themes.get(chosen_name) {
        tm_theme = Some(embedded_theme.clone());
    }

    // 3. Derive the UI theme dynamically!
    if let Some(t) = &tm_theme {
        final_theme = derive_ui_theme(t);
        fallback_tm = t.clone();
    }

    // 4. Finally, apply specific manual overrides from the TOML if present
    if let Some(cfg) = theme_cfg {
        let get_col = |key: &str, default: ratatui::style::Color| -> ratatui::style::Color {
            theme::resolve_color(cfg.colors.get(key), tm_theme.as_ref(), default)
        };

        final_theme.bg = get_col("bg", final_theme.bg);
        final_theme.cursor_bg = get_col("cursor_bg", final_theme.cursor_bg);
        final_theme.fg = get_col("fg", final_theme.fg);
        final_theme.dim = get_col("dim", final_theme.dim);
        final_theme.staged = get_col("staged", final_theme.staged);
        final_theme.unstaged = get_col("unstaged", final_theme.unstaged);
        final_theme.partial = get_col("partial", final_theme.partial);
        final_theme.dir = get_col("dir", final_theme.dir);
        final_theme.cmd = get_col("cmd", final_theme.cmd);
        final_theme.add_bg = get_col("add_bg", final_theme.add_bg);
        final_theme.del_bg = get_col("del_bg", final_theme.del_bg);
        final_theme.add_fg = get_col("add_fg", final_theme.add_fg);
        final_theme.del_fg = get_col("del_fg", final_theme.del_fg);
    }

    Ok((final_theme, fallback_tm))
}

pub fn watch_config(
    config_path: PathBuf,
    tx: tokio::sync::mpsc::Sender<(UiTheme, syntect::highlighting::Theme)>,
) {
    tokio::spawn(async move {
        let mut last_mtime = None;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));

        loop {
            interval.tick().await;

            if let Ok(meta) = std::fs::metadata(&config_path) {
                if let Ok(mtime) = meta.modified() {
                    if Some(mtime) != last_mtime {
                        last_mtime = Some(mtime);

                        let theme_opt = load_config_and_theme(&config_path).ok();
                        if let Some(theme) = theme_opt {
                            let _ = tx.send(theme).await;
                        }
                    }
                }
            }
        }
    });
}
