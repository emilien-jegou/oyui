use parking_lot::RwLock;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syntect::highlighting::Theme;
use tracing::{info, info_span};

pub mod builtin;
pub mod define_default_theme;
pub mod script;
pub mod theme;

pub use builtin::{fallback_theme, get_embedded_themes};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, UiTheme};

use crate::actions::BoxedHandler;
use crate::commons::lazy::Lazy;
use crate::worker::tasks::watch_config;
use crate::worker::EventRegistry;

thread_local! {
    pub static ACTIVE_REGISTRY: RefCell<crate::actions::keybinds::KeybindRegistry> =
        RefCell::new(crate::actions::keybinds::default_keybinds());
}

pub fn clear_registry() {
    ACTIVE_REGISTRY.with(|r| {
        *r.borrow_mut() = crate::actions::keybinds::default_keybinds();
    });
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

pub struct Config {
    pub path: PathBuf,
    pub theme: Lazy<UiTheme>,
    pub syntax_theme: Arc<RwLock<Lazy<Theme>>>,
}

impl Config {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            theme: Lazy::Uninitialized,
            syntax_theme: Arc::new(RwLock::new(Lazy::Uninitialized)),
        }
    }

    pub fn start_watching(&self, worker: &EventRegistry) -> eyre::Result<()> {
        worker.send(watch_config::WatchConfigReq {
            path: self.path.clone(),
            last_mtime: None,
        })?;
        Ok(())
    }

    pub fn update_theme(&mut self, ui_theme: &UiTheme, tm_theme: &Theme) {
        self.theme = Lazy::Ready(ui_theme.clone());
        *self.syntax_theme.write() = Lazy::Ready(tm_theme.clone());
    }
}
