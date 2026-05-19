use ratatui::style::Color as TuiColor;
use std::sync::Arc;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

pub struct SyntaxEngine {
    pub syntax_set: Arc<SyntaxSet>,
    pub theme: Arc<Theme>,
}

impl SyntaxEngine {
    pub fn new() -> Self {
        let syntax_set = Arc::new(two_face::syntax::extra_newlines());
        // Load default theme (you'd replace this with your embedded loading logic)
        let mut theme = ThemeSet::load_defaults().themes["base16-ocean.dark"].clone();

        // Critical: Strip backgrounds so UI colors show through
        strip_theme_backgrounds(&mut theme);

        Self {
            syntax_set,
            theme: Arc::new(theme),
        }
    }
}

fn strip_theme_backgrounds(theme: &mut Theme) {
    theme.settings.background = None;
    for item in &mut theme.scopes {
        item.style.background = None;
    }
}
