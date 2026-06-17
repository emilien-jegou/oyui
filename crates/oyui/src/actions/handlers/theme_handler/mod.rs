use std::sync::Arc;

use crate::actions::state::{ansi_default_theme, TuiState};
use crate::config::{self, LineHighlightMode};
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::worker::events::theme_update::ThemeUpdate;
use crate::worker::EventRegistry;
use crate::{actions::*, view};

pub mod macros;
pub mod utils;

#[derive(Clone)]
pub struct AppThemeActionsHandler {
    pub state: Arc<TuiState>,
    pub cache: DiffCache,
    pub view: view::View,
    pub color_mode: TerminalColorMode,
    pub worker: Arc<EventRegistry>,
}

impl ThemeActionsHandler for AppThemeActionsHandler {
    fn set(&self, name: String) {
        let (base_ui, tm) = if let Some(path_str) = name.strip_prefix("path:") {
            match std::fs::File::open(path_str) {
                Ok(file) => {
                    let mut reader = std::io::BufReader::new(file);
                    match syntect::highlighting::ThemeSet::load_from_reader(&mut reader) {
                        Ok(tm_theme) => {
                            let ui_theme = crate::config::derive_ui_theme(&tm_theme);
                            (ui_theme, Some(tm_theme))
                        }
                        Err(e) => {
                            tracing::error!("Failed to parse tmTheme at '{}': {}", path_str, e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to open tmTheme file at '{}': {}", path_str, e);
                    return;
                }
            }
        } else if name == "ansi" {
            (ansi_default_theme(&self.color_mode), None)
        } else {
            match config::get_theme(&name) {
                Some(t) => t,
                None => {
                    tracing::error!("Invalid theme name given: {}", name);
                    return;
                }
            }
        };

        let mut theme = self.state.theme.write();
        theme.ui = base_ui.clone();
        theme.tm_theme = tm.clone();
        let _ = self.worker.send(ThemeUpdate::Full(base_ui, tm));
    }

    fn toggle_gradient(&self) {
        unimplemented!();
    }

    fn syntax(&self, name: String) {
        let tm = if let Some(path_str) = name.strip_prefix("path:") {
            match std::fs::File::open(path_str) {
                Ok(file) => {
                    let mut reader = std::io::BufReader::new(file);
                    match syntect::highlighting::ThemeSet::load_from_reader(&mut reader) {
                        Ok(tm_theme) => Some(tm_theme),
                        Err(e) => {
                            tracing::error!("Failed to parse tmTheme at '{}': {}", path_str, e);
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to open tmTheme file at '{}': {}", path_str, e);
                    return;
                }
            }
        } else {
            match config::get_theme(&name) {
                Some(t) => t.1,
                None => {
                    tracing::error!("Invalid theme name given: {}", name);
                    return;
                }
            }
        };

        let mut theme = self.state.theme.write();
        theme.tm_theme = tm.clone();
        let _ = self.worker.send(ThemeUpdate::Tm(tm));
    }

    fn is_dark(&self) -> bool {
        let theme = self.state.theme.read();
        theme.ui.bg.is_dark()
    }
}

// Color fields
macros::impl_color_getset!(bg);
macros::impl_color_getset!(fg);
macros::impl_color_getset!(cursor_bg);
macros::impl_color_getset!(dim);
macros::impl_color_getset!(staged);
macros::impl_color_getset!(unstaged);
macros::impl_color_getset!(partial);
macros::impl_color_getset!(dir);
macros::impl_color_getset!(cmd);
macros::impl_color_getset!(add_bg);
macros::impl_color_getset!(del_bg);
macros::impl_color_getset!(add_fg);
macros::impl_color_getset!(del_fg);
macros::impl_color_getset!(char_scroll_fg);
macros::impl_color_getset!(char_trailing_space_fg);
macros::impl_color_getset!(char_tab_fg);
macros::impl_opt_color_getset!(char_line_split_color);
macros::impl_opt_color_getset!(char_hunk_split_color);

// String fields
macros::impl_ty_getset!(tree_progressive_change_dim, bool);
macros::impl_ty_getset!(char_line_split, String);
macros::impl_ty_getset!(char_hunk_split, String);
macros::impl_ty_getset!(char_indicator, String);
macros::impl_ty_getset!(char_add_sign, String);
macros::impl_ty_getset!(char_del_sign, String);
macros::impl_ty_getset!(char_scroll_both, String);
macros::impl_ty_getset!(char_scroll_right, String);
macros::impl_ty_getset!(char_scroll_left, String);
macros::impl_ty_getset!(char_trailing_space, String);
macros::impl_ty_getset!(char_tab, String);

// Highlight modes
impl ThemeFileStagedHighlightActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.state.theme.read().ui.file_staged_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.state.theme.write().ui.file_staged_highlight = val;
    }
}

impl ThemeFileStagedHighlightOpacityActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> f64 {
        self.state.theme.read().ui.file_staged_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.state.theme.write().ui.file_staged_highlight_opacity = val;
    }
}

impl ThemeFileChangeHighlightActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.state.theme.read().ui.file_change_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.state.theme.write().ui.file_change_highlight = val;
    }
}

impl ThemeFileChangeHighlightOpacityActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> f64 {
        self.state.theme.read().ui.file_change_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.state.theme.write().ui.file_change_highlight_opacity = val;
    }
}
