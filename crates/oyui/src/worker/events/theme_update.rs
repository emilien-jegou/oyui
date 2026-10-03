//! Notification that the UI or syntect theme changed, carrying the open file.

use std::path::PathBuf;

use crate::config::UiTheme;
use syntect::highlighting::Theme;

/// Theme change plus the file open in the view, so listeners never touch UI state.
#[derive(Clone)]
pub enum ThemeUpdate {
    /// Whole UI theme replaced; carries the syntect theme and the open file.
    Full(UiTheme, Option<Theme>, Option<PathBuf>),
    /// Only the syntect highlighting theme changed; carries the open file.
    Tm(Option<Theme>, Option<PathBuf>),
}
