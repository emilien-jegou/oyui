use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod builtin;
pub mod define_default_theme;
pub mod theme;

pub use builtin::{fallback_theme, get_embedded_themes};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, ThemeConfig, UiTheme};

// Keep this out of the macro file to avoid having ratatui as build deps
impl From<theme::Color> for ratatui::style::Color {
    fn from(c: theme::Color) -> Self {
        match c {
            theme::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        }
    }
}

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

    let runtime_theme_cfg = app_config.theme.get(chosen_name);

    let mut final_theme;
    let final_tm_theme;

    if let Some(path) = runtime_theme_cfg.and_then(|c| c.tm_theme_path.as_ref()) {
        let t = syntect::highlighting::ThemeSet::get_theme(path)?;
        final_theme = derive_ui_theme(&t);
        final_tm_theme = t;
    } else if let Some((emb_ui, emb_tm)) = get_embedded_themes().get(chosen_name) {
        final_theme = emb_ui.clone();
        final_tm_theme = emb_tm.clone();
    } else {
        let (ui, tm) = fallback_theme();
        final_theme = ui;
        final_tm_theme = tm;
    }

    if let Some(cfg) = runtime_theme_cfg {
        final_theme.apply_overrides(&cfg.colors, Some(&final_tm_theme));
    }

    Ok((final_theme, final_tm_theme))
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
