use crate::app::CommandMode;
use crate::config::{fallback_theme, UiTheme};

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
    pub theme: ThemeState,
    pub should_quit: bool,
    pub command_mode: CommandMode,
    pub confirm_merge_window_enabled: bool,
}

impl TuiState {
    pub fn new(base_theme_name: &str) -> Self {
        Self {
            theme: ThemeState::new(base_theme_name),
            should_quit: false,
            command_mode: CommandMode::Normal,
            confirm_merge_window_enabled: false,
        }
    }
}
