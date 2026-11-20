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
use crate::app::SessionContext;
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
///
/// Shared state (error cell, worker handle) comes from the session context
/// through `Deref` — this struct owns only what is genuinely per-config.
pub struct Config {
    /// The config file's path on disk.
    pub path: PathBuf,
    /// Owns the engine and every script-defined callback.
    pub host: RuneHost,
    /// Keybinds installed by the script host; read on every key press.
    pub keybinds: KeybindRegistry,
    /// The dispatch table built from the session's action handlers.
    pub handler: BoxedHandler,
    /// The shared session state; the single owner of the shared handles.
    pub ctx: Arc<SessionContext>,
}

impl std::ops::Deref for Config {
    type Target = SessionContext;

    fn deref(&self) -> &SessionContext {
        &self.ctx
    }
}

impl Config {
    pub fn start_watching(&self, worker: &EventRegistry) -> eyre::Result<()> {
        // Seed the watcher with the file's current mtime: the config was just
        // bootstrapped synchronously, so the watcher's first pass must not
        // re-fire the same revision (finding 4.1).
        let last_mtime = std::fs::metadata(&self.path)
            .and_then(|meta| meta.modified())
            .ok();
        worker.send(watch_config::WatchConfigReq {
            path: self.path.clone(),
            last_mtime,
        })?;
        Ok(())
    }

    /// The services a load needs: the dispatcher the script's actions
    /// resolve against and the worker async natives send through.
    fn script_context(&self) -> crate::script::ScriptContext {
        crate::script::ScriptContext {
            handler: self.handler.clone(),
            worker: Some(self.worker.clone()),
        }
    }

    /// Synchronously loads the config before the first frame paints — the
    /// explicit startup bootstrap (finding 4.1). The script's keybinds are
    /// in effect from the first keypress instead of after the watcher's
    /// first event.
    pub fn bootstrap(&mut self) {
        let outcome = self.host.load(&self.path, &self.script_context());
        self.apply(outcome);
    }

    /// Installs a load outcome: adopts the keybinds and surfaces errors. A
    /// successful load then fires the script's `config_reload` hook.
    fn apply(&mut self, outcome: crate::script::ScriptLoad) {
        if let Some(keybinds) = outcome.keybinds {
            self.keybinds = keybinds;
        }
        match outcome.error {
            Some(e) => {
                tracing::error!("Config error: {}", e);
                *self.error.write() = Some(e.to_string());
            }
            None => {
                // Clear a stale error once a load succeeds.
                *self.error.write() = None;
                if let Err(e) = self.host.call_event("config_reload") {
                    *self.error.write() = Some(e.to_string());
                }
            }
        }
    }

    /// Reacts to a file change: kicks off the off-thread recompile; the
    /// result is applied by [`Self::apply_parked_reload`] on a later frame
    /// (finding 4.1: the Rune compile no longer blocks the render loop).
    pub fn handle_reload_event(&mut self, path: &Path) {
        self.host.reload(path.to_path_buf(), &self.script_context());
    }

    /// Applies a finished reload, if one landed since the last frame.
    ///
    /// Called from the event loop between frames: nothing here blocks.
    pub fn apply_parked_reload(&mut self) {
        if !self.host.has_parked_reload() {
            return;
        }
        let (keybinds, error) = self.host.apply_parked();
        self.apply(crate::script::ScriptLoad { keybinds, error });
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
