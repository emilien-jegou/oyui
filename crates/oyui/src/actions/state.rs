use parking_lot::RwLock;

use crate::app::CommandMode;
use crate::config::{fallback_theme, UiTheme};
use std::sync::atomic::AtomicBool;

pub struct ThemeState {
    pub ui: UiTheme,
    pub base_theme_name: String,
    pub tm_theme: syntect::highlighting::Theme,
}

impl ThemeState {
    pub fn new(base_theme_name: &str) -> Self {
        let (ui, tm_theme) = fallback_theme(base_theme_name);
        Self {
            base_theme_name: base_theme_name.to_string(),
            ui,
            tm_theme,
        }
    }
}

pub struct TuiState {
    pub theme: RwLock<ThemeState>,
    pub should_quit: AtomicBool,
    pub command_mode: RwLock<CommandMode>,
    pub confirm_merge_window_enabled: AtomicBool,
}

impl TuiState {
    pub fn new(base_theme_name: &str) -> Self {
        Self {
            theme: RwLock::new(ThemeState::new(base_theme_name)),
            should_quit: AtomicBool::new(false),
            command_mode: RwLock::new(CommandMode::Normal),
            confirm_merge_window_enabled: AtomicBool::new(false),
        }
    }
}
