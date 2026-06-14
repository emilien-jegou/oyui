use parking_lot::RwLock;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, info_span};

pub mod builtin;
pub mod define_default_theme;
pub mod script;
pub mod theme;

pub use builtin::{get_theme, get_embedded_themes};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, UiTheme};

use crate::actions::BoxedHandler;
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
            theme::Color::Ansi(v) => ratatui::style::Color::Indexed(v),
            theme::Color::Ansi256(v) => ratatui::style::Color::Indexed(v),
            theme::Color::Reset => ratatui::style::Color::Reset,
            theme::Color::Bg => ratatui::style::Color::Reset,
            theme::Color::Fg => ratatui::style::Color::Reset,
            theme::Color::Black => ratatui::style::Color::Black,
            theme::Color::Red => ratatui::style::Color::Red,
            theme::Color::Green => ratatui::style::Color::Green,
            theme::Color::Yellow => ratatui::style::Color::Yellow,
            theme::Color::Blue => ratatui::style::Color::Blue,
            theme::Color::Magenta => ratatui::style::Color::Magenta,
            theme::Color::Cyan => ratatui::style::Color::Cyan,
            theme::Color::Gray => ratatui::style::Color::Gray,
            theme::Color::DarkGray => ratatui::style::Color::DarkGray,
            theme::Color::LightRed => ratatui::style::Color::LightRed,
            theme::Color::LightGreen => ratatui::style::Color::LightGreen,
            theme::Color::LightYellow => ratatui::style::Color::LightYellow,
            theme::Color::LightBlue => ratatui::style::Color::LightBlue,
            theme::Color::LightMagenta => ratatui::style::Color::LightMagenta,
            theme::Color::LightCyan => ratatui::style::Color::LightCyan,
            theme::Color::White => ratatui::style::Color::White,
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

    let context = script::build_context(handler)?;
    let mut vm = script::build_vm(path, context)?;
    script::run_config_script(&mut vm)?;

    info!("Config loaded successfully");
    Ok(())
}

pub struct Config {
    pub path: PathBuf,
    pub error: Arc<RwLock<Option<String>>>,
    pub handler: BoxedHandler,
}

impl Config {
    pub fn new(path: PathBuf, handler: BoxedHandler, error: Arc<RwLock<Option<String>>>) -> Self {
        Self {
            path,
            error,
            handler,
        }
    }

    pub fn start_watching(&self, worker: &EventRegistry) -> eyre::Result<()> {
        worker.send(watch_config::WatchConfigReq {
            path: self.path.clone(),
            last_mtime: None,
        })?;
        Ok(())
    }

    pub fn handle_reload_event(&mut self, path: &Path) {
        tracing::info!("Reloading config on main thread...");
        if let Err(e) = load_config(path, self.handler.clone()) {
            tracing::error!("Config compilation error: {}", e);
            *self.error.write() = Some(e.to_string());
        } else {
            *self.error.write() = None;
        }
    }
}
