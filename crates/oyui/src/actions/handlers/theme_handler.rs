use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::config::theme::Color;
use crate::config::LineHighlightMode;

/// Helper to convert an internal [`Color`] enum back into a `#rrggbb` hex string
fn color_to_hex(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
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

macro_rules! impl_opt_color_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppActionsHandler {
                fn get(&self) -> String {
                    if let Some(color) = self.state.read().theme.ui.$field {
                        color_to_hex(color)
                    } else {
                        String::new()
                    }
                }

                fn set(&self, val: String) {
                    if val.is_empty() || val == "none" {
                        self.state.write().theme.ui.$field = None;
                    } else if let Some(c) = crate::actions::parse_hex_color(&val) {
                        self.state.write().theme.ui.$field = Some(c);
                    }
                }
            }
        }
    };
}

macro_rules! impl_color_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppActionsHandler {
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
        }
    };
}

macro_rules! impl_string_getset {
    ($field:ident) => {
        paste::paste! {
            impl [< Theme $field:camel ActionsHandler >] for AppActionsHandler {
                fn get(&self) -> String {
                    self.state.read().theme.ui.$field.clone()
                }

                fn set(&self, val: String) {
                    self.state.write().theme.ui.$field = val;
                }
            }
        }
    };
}

// Color fields
impl_color_getset!(bg);
impl_color_getset!(fg);
impl_color_getset!(cursor_bg);
impl_color_getset!(dim);
impl_color_getset!(staged);
impl_color_getset!(unstaged);
impl_color_getset!(partial);
impl_color_getset!(dir);
impl_color_getset!(cmd);
impl_color_getset!(add_bg);
impl_color_getset!(del_bg);
impl_color_getset!(add_fg);
impl_color_getset!(del_fg);
impl_opt_color_getset!(char_line_split_color);
impl_opt_color_getset!(char_hunk_split_color);
impl_color_getset!(char_scroll_fg);
impl_color_getset!(char_trailing_space_fg);
impl_color_getset!(char_tab_fg);

// String fields
impl_string_getset!(char_line_split);
impl_string_getset!(char_hunk_split);
impl_string_getset!(char_indicator);
impl_string_getset!(char_add_sign);
impl_string_getset!(char_del_sign);
impl_string_getset!(char_scroll_both);
impl_string_getset!(char_scroll_right);
impl_string_getset!(char_scroll_left);
impl_string_getset!(char_trailing_space);
impl_string_getset!(char_tab);

// Highlight modes
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
