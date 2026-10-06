use parking_lot::{Mutex, RwLock};
use std::sync::Arc;

use crate::actions::*;
use crate::app::UiState;
use crate::config::{self, LineHighlightMode};
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::theme::{ansi_default_theme, ThemeState};
use crate::worker::events::theme_update::ThemeUpdate;
use crate::worker::EventRegistry;

pub mod macros;
pub mod utils;

#[derive(Clone)]
pub struct AppThemeActionsHandler {
    pub theme: Arc<RwLock<ThemeState>>,
    pub cache: DiffCache,
    pub ui: Arc<Mutex<UiState>>,
    pub color_mode: TerminalColorMode,
    pub worker: Arc<EventRegistry>,
    /// Shared config-error cell; action failures surface through it.
    pub error: Arc<RwLock<Option<String>>>,
}

impl AppThemeActionsHandler {
    /// Records a script-visible failure so the config overlay can show it.
    fn fail(&self, message: impl std::fmt::Display) {
        tracing::error!("{message}");
        *self.error.write() = Some(format!("theme: {message}"));
    }

    /// Broadcasts the current UI theme so workers (syntax cache) stay live.
    /// Call after any in-place `theme.ui` mutation.
    fn notify_ui_changed(&self) {
        let (ui, tm) = {
            let theme = self.theme.read();
            (theme.ui.clone(), theme.tm_theme.clone())
        };
        let open_file = self.ui.lock().file_view.current_path.clone();
        let _ = self.worker.send(ThemeUpdate::Full(ui, tm, open_file));
    }
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
                            self.fail(format!("failed to parse tmTheme at '{path_str}': {e}"));
                            return;
                        }
                    }
                }
                Err(e) => {
                    self.fail(format!("failed to open tmTheme file at '{path_str}': {e}"));
                    return;
                }
            }
        } else if name == "ansi" {
            (ansi_default_theme(&self.color_mode), None)
        } else {
            match config::get_theme(&name) {
                Some(t) => t,
                None => {
                    self.fail(format!("invalid theme name '{name}'"));
                    return;
                }
            }
        };

        let open_file = self.ui.lock().file_view.current_path.clone();
        let mut theme = self.theme.write();
        theme.ui = base_ui.clone();
        theme.tm_theme = tm.clone();
        theme.name = Some(name.clone());
        let _ = self.worker.send(ThemeUpdate::Full(base_ui, tm, open_file));
    }

    fn toggle_gradient(&self) {
        let mut ui = self.ui.lock();
        ui.file_view.use_gradient = !ui.file_view.use_gradient;
        drop(ui);
        self.notify_ui_changed();
    }

    fn syntax(&self, name: String) {
        let tm = if let Some(path_str) = name.strip_prefix("path:") {
            match std::fs::File::open(path_str) {
                Ok(file) => {
                    let mut reader = std::io::BufReader::new(file);
                    match syntect::highlighting::ThemeSet::load_from_reader(&mut reader) {
                        Ok(tm_theme) => Some(tm_theme),
                        Err(e) => {
                            self.fail(format!("failed to parse tmTheme at '{path_str}': {e}"));
                            return;
                        }
                    }
                }
                Err(e) => {
                    self.fail(format!("failed to open tmTheme file at '{path_str}': {e}"));
                    return;
                }
            }
        } else {
            match config::get_theme(&name) {
                Some(t) => t.1,
                None => {
                    self.fail(format!("invalid syntax theme name '{name}'"));
                    return;
                }
            }
        };

        let open_file = self.ui.lock().file_view.current_path.clone();
        let mut theme = self.theme.write();
        theme.tm_theme = tm.clone();
        let _ = self.worker.send(ThemeUpdate::Tm(tm, open_file));
    }

    fn is_dark(&self) -> bool {
        let theme = self.theme.read();
        theme.ui.bg.is_dark()
    }

    fn name(&self) -> String {
        self.theme.read().name.clone().unwrap_or_default()
    }

    fn list(&self) -> String {
        let mut names: Vec<&str> = config::get_embedded_themes()
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        names.join("\n")
    }
}

/// Toggles gradient line rendering for the file view.
impl ThemeGradientActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> bool {
        self.ui.lock().file_view.use_gradient
    }

    fn set(&self, val: bool) {
        self.ui.lock().file_view.use_gradient = val;
        self.notify_ui_changed();
    }
}

// Color fields
macros::impl_color_getset!(bg);
macros::impl_color_getset!(fg);
macros::impl_color_getset!(cursor_bg);
macros::impl_color_getset!(dim);
macros::impl_color_getset!(dimmer);
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
macros::impl_color_getset!(conflict_fg);
macros::impl_opt_color_getset!(conflict_bg);

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
        self.theme.read().ui.file_staged_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.theme.write().ui.file_staged_highlight = val;
        self.notify_ui_changed();
    }
}

impl ThemeFileStagedHighlightOpacityActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> f64 {
        self.theme.read().ui.file_staged_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.theme.write().ui.file_staged_highlight_opacity = val;
        self.notify_ui_changed();
    }
}

impl ThemeFileChangeHighlightActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.theme.read().ui.file_change_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.theme.write().ui.file_change_highlight = val;
        self.notify_ui_changed();
    }
}

impl ThemeFileChangeHighlightOpacityActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> f64 {
        self.theme.read().ui.file_change_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.theme.write().ui.file_change_highlight_opacity = val;
        self.notify_ui_changed();
    }
}

impl ThemeFileConflictHighlightActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.theme.read().ui.file_conflict_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.theme.write().ui.file_conflict_highlight = val;
        self.notify_ui_changed();
    }
}

impl ThemeFileConflictHighlightOpacityActionsHandler for AppThemeActionsHandler {
    fn get(&self) -> f64 {
        self.theme.read().ui.file_conflict_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.theme.write().ui.file_conflict_highlight_opacity = val;
        self.notify_ui_changed();
    }
}
