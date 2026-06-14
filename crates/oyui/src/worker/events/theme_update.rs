use crate::config::UiTheme;
use syntect::highlighting::Theme;

#[derive(Clone)]
pub struct ThemeUpdate {
    pub ui: UiTheme,
    pub tm: Option<Theme>,
}

impl std::fmt::Debug for ThemeUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeUpdate").finish()
    }
}
