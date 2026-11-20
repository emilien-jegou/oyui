//! Script engine boundary: the only module allowed to name a script engine.
//!
//! Everything else in the app depends on [`ScriptHost`] and [`ScriptError`];
//! engine details stay behind them.

mod compile;
mod highlight;
mod host;
pub mod language_server;
mod natives;
#[cfg(test)]
mod tests;

pub use host::RuneHost;

use crate::actions::keybinds::KeybindRegistry;
use crate::actions::BoxedHandler;
use std::error::Error;
use std::fmt;
use std::path::Path;

pub use crate::commons::ids::CallbackId;

/// The app services a script host needs at load time.
///
/// One bundle instead of port parameters: the dispatch table async actions
/// resolve against and the worker async natives send through. Engines stay
/// behind them — the port reads neither by name.
pub struct ScriptContext {
    /// The dispatch table registered actions resolve against.
    pub handler: BoxedHandler,
    /// The event worker async natives send through; `None` in hostless
    /// contexts like the language server.
    pub worker: Option<std::sync::Arc<crate::worker::EventRegistry>>,
}

/// Failure reported by a [`ScriptHost`].
#[derive(Debug, Clone)]
pub struct ScriptError {
    /// Human-readable text; compiler output may carry ANSI colour.
    pub message: String,
}

impl ScriptError {
    /// Wraps `message` as the host's failure report.
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ScriptError {}

/// Outcome of a synchronous load.
///
/// The only synchronous load is the startup bootstrap; reloads compile off
/// thread and are applied by [`crate::config::Config::apply_parked_reload`].
pub struct ScriptLoad {
    /// Keybinds the script produced; `None` leaves current ones alone.
    pub keybinds: Option<KeybindRegistry>,
    /// Set when compilation or execution failed.
    pub error: Option<ScriptError>,
}

/// The port the application depends on; implementations hide their engine.
///
/// The port carries no threading model: async script callbacks park in the
/// engine's own plumbing (see [`PendingScriptResult`]), and config reloads
/// are engine-specific off-thread work.
pub trait ScriptHost {
    /// Compiles and runs `path` synchronously, collecting the keybinds it
    /// registers.
    fn load(&mut self, path: &Path, ctx: &ScriptContext) -> ScriptLoad;
    /// Runs a callback the script registered with `keybind`.
    fn call(&self, id: CallbackId) -> Result<(), ScriptError>;
    /// Runs a named command the script registered with `command::register`.
    fn call_command(&self, name: &str, args: &str) -> Result<(), ScriptError>;
    /// Runs every callback registered for `event` with `on`.
    fn call_event(&self, event: &str) -> Result<(), ScriptError>;
}

/// A script callback waiting on an off-thread result.
///
/// It owns the reply the registry handed to whoever asked, so a request no
/// longer has to be matched by an id afterwards — and the wait dies with the
/// config that issued it, instead of leaving a stale entry behind for a
/// result that later arrives to find its callback gone.
pub trait PendingScriptResult: Send {
    /// Delivers the result if it has arrived, consuming the callback.
    fn try_deliver(&mut self) -> Option<Result<(), ScriptError>>;
}
