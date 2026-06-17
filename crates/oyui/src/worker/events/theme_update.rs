use crate::config::UiTheme;
use syntect::highlighting::Theme;

#[derive(Clone)]
pub enum ThemeUpdate {
    Full(UiTheme, Option<Theme>),
    Tm(Option<Theme>)
}

impl std::fmt::Debug for ThemeUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThemeUpdate").finish()
    }
}
