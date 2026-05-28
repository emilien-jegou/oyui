use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::config::theme::Color;
use crate::config::LineHighlightMode;

/// Helper to convert an internal [`Color`] enum back into a `#rrggbb` hex string
fn color_to_hex(c: Color) -> String {
    #[allow(unreachable_patterns)]
    match c {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => "#000000".to_string(),
    }
}

impl ThemeActionsHandler for AppActionsHandler {
    fn get(&self) -> String {
        self.state.read().theme.base_theme_name.clone()
    }

    fn set(&self, name: String) {
        let mut state = self.state.write();
        let s = &state.theme.base_theme_name;
        let (base_ui, tm) = crate::config::get_embedded_themes()
            .get(&name)
            .cloned()
            .unwrap_or_else(|| crate::config::fallback_theme(s));
        state.theme.ui = base_ui;
        state.theme.base_theme_name = name;
        state.theme.tm_theme = tm;
    }

    fn toggle_gradient(&self) {
        unimplemented!();
    }
}

// Macro to implement repetitive Color setters and getters cleanly
macro_rules! impl_color_getset {
    ($trait_name:ident, $field:ident) => {
        impl $trait_name for AppActionsHandler {
            fn get(&self) -> String {
                let color = self.state.read().theme.ui.$field;
                color_to_hex(color)
            }

            fn set(&self, val: String) {
                if let Some(c) = crate::actions::parse_hex_color(&val) {
                    self.state.write().theme.ui.$field = c;
                }
            }
        }
    };
}

impl_color_getset!(ThemeBgActionsHandler, bg);
impl_color_getset!(ThemeFgActionsHandler, fg);
impl_color_getset!(ThemeCursorBgActionsHandler, cursor_bg);
impl_color_getset!(ThemeDimActionsHandler, dim);
impl_color_getset!(ThemeStagedActionsHandler, staged);
impl_color_getset!(ThemeUnstagedActionsHandler, unstaged);
impl_color_getset!(ThemePartialActionsHandler, partial);
impl_color_getset!(ThemeDirActionsHandler, dir);
impl_color_getset!(ThemeCmdActionsHandler, cmd);
impl_color_getset!(ThemeAddBgActionsHandler, add_bg);
impl_color_getset!(ThemeDelBgActionsHandler, del_bg);
impl_color_getset!(ThemeAddFgActionsHandler, add_fg);
impl_color_getset!(ThemeDelFgActionsHandler, del_fg);

impl ThemeFileStagedHighlightActionsHandler for AppActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.state.read().theme.ui.file_staged_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.state.write().theme.ui.file_staged_highlight = val;
    }
}

impl ThemeFileStagedHighlightOpacityActionsHandler for AppActionsHandler {
    fn get(&self) -> f64 {
        self.state.read().theme.ui.file_staged_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.state.write().theme.ui.file_staged_highlight_opacity = val;
    }
}

impl ThemeFileChangeHighlightActionsHandler for AppActionsHandler {
    fn get(&self) -> LineHighlightMode {
        self.state.read().theme.ui.file_change_highlight
    }

    fn set(&self, val: LineHighlightMode) {
        self.state.write().theme.ui.file_change_highlight = val;
    }
}

impl ThemeFileChangeHighlightOpacityActionsHandler for AppActionsHandler {
    fn get(&self) -> f64 {
        self.state.read().theme.ui.file_change_highlight_opacity
    }

    fn set(&self, val: f64) {
        self.state.write().theme.ui.file_change_highlight_opacity = val;
    }
}
