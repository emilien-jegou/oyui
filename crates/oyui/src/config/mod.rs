use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

pub mod builtin;
pub mod define_default_theme;
pub mod theme;

pub use builtin::{get_embedded_themes, get_theme};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, UiTheme};

use crate::actions::keybinds::KeybindRegistry;
use crate::actions::BoxedHandler;
use crate::script::{RuneHost, ScriptHost};
use crate::worker::tasks::watch_config;
use crate::worker::EventRegistry;

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

/// Script-backed app configuration: the config file, its script host, and the
/// keybinds the script produced.
pub struct Config {
    pub path: PathBuf,
    pub error: Arc<RwLock<Option<String>>>,
    pub handler: BoxedHandler,
    /// Keybinds installed by the script host; read on every key press.
    pub keybinds: KeybindRegistry,
    /// Owns the engine and every script-defined callback.
    pub host: RuneHost,
    /// Worker handle used by async script natives.
    pub worker: Arc<EventRegistry>,
}

impl Config {
    pub fn start_watching(&self, worker: &EventRegistry) -> eyre::Result<()> {
        worker.send(watch_config::WatchConfigReq {
            path: self.path.clone(),
            last_mtime: None,
        })?;
        Ok(())
    }

    /// Recompiles the config script and swaps in whatever it produced.
    pub fn handle_reload_event(&mut self, path: &Path) {
        info!("Reloading config on main thread...");
        let outcome = self
            .host
            .load(path, self.handler.clone(), Some(self.worker.clone()));
        if let Some(keybinds) = outcome.keybinds {
            self.keybinds = keybinds;
        }
        match outcome.error {
            Some(e) => {
                tracing::error!("Config compilation error: {}", e);
                *self.error.write() = Some(e.to_string());
                return;
            }
            None => *self.error.write() = None,
        }

        if let Err(e) = self.host.call_event("config_reload") {
            *self.error.write() = Some(e.to_string());
        }
    }

    /// Runs a callback the script registered through `keybind`.
    pub fn call_callback(
        &self,
        id: crate::script::CallbackId,
    ) -> Result<(), crate::script::ScriptError> {
        self.host.call(id)
    }

    /// Runs a named command the script registered through `command::register`.
    pub fn call_command(&self, name: &str, args: &str) -> Result<(), crate::script::ScriptError> {
        self.host.call_command(name, args)
    }

    /// Runs every callback the script registered for `event` through `on`.
    pub fn call_event(&self, event: &str) -> Result<(), crate::script::ScriptError> {
        self.host.call_event(event)
    }

    /// Delivers every off-thread result that has arrived; returns what failed.
    ///
    /// Called from the event loop between frames: nothing here blocks, so a
    /// slow request costs nothing while it runs.
    pub fn drain_pending(&self) -> Vec<crate::script::ScriptError> {
        self.host.drain_pending()
    }
}
