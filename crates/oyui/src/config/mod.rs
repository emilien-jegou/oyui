use std::cell::RefCell;
use std::path::Path;
use tracing::{info, info_span};

pub mod builtin;
pub mod define_default_theme;
pub mod script;
pub mod theme;

pub use builtin::{fallback_theme, get_embedded_themes};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, UiTheme};

use crate::actions::BoxedHandler;

thread_local! {
    pub static ACTIVE_REGISTRY: RefCell<crate::actions::keybinds::KeybindRegistry> =
        RefCell::new(crate::actions::keybinds::default_keybinds());
}

impl From<theme::Color> for ratatui::style::Color {
    fn from(c: theme::Color) -> Self {
        match c {
            theme::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        }
    }
}

/// Load and execute a `.rn` config script.
pub fn load_config(path: &Path, handler: BoxedHandler) -> Result<(), Box<dyn std::error::Error>> {
    let span = info_span!("load_config", path = %path.display());
    let _enter = span.enter();

    if !path.exists() {
        info!("Config file is absent. Using fallback theme.");
        return Ok(());
    }

    info!("Config file found. Preparing to compile.");

    ACTIVE_REGISTRY.with(|r| *r.borrow_mut() = crate::actions::keybinds::default_keybinds());

    let mut vm = script::build_vm(path, handler)?;
    script::run_config_script(&mut vm)?;

    info!("Config loaded successfully");
    Ok(())
}
