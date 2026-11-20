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

/// Opaque handle to a callback the script registered through `keybind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId(pub(crate) u64);

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

/// Outcome of loading a config script.
pub struct ScriptLoad {
    /// Keybinds the script produced; `None` leaves current ones alone.
    pub keybinds: Option<KeybindRegistry>,
    /// Set when compilation or execution failed.
    pub error: Option<ScriptError>,
}

/// The port the application depends on; implementations hide their engine.
pub trait ScriptHost {
    /// Compiles and runs `path`, collecting the keybinds it registers.
    fn load(
        &mut self,
        path: &Path,
        handler: BoxedHandler,
        worker: Option<std::sync::Arc<crate::worker::EventRegistry>>,
    ) -> ScriptLoad;
    /// Runs a callback the script registered with `keybind`.
    fn call(&self, id: CallbackId) -> Result<(), ScriptError>;
    /// Runs a named command the script registered with `command::register`.
    fn call_command(&self, name: &str, args: &str) -> Result<(), ScriptError>;
    /// Runs every callback registered for `event` with `on`.
    fn call_event(&self, event: &str) -> Result<(), ScriptError>;
    /// Parks a callback that is waiting on an off-thread result.
    fn park(&self, task: Box<dyn PendingScriptResult>);
    /// Delivers every result that has arrived; returns what went wrong.
    fn drain_pending(&self) -> Vec<ScriptError>;
}

/// A script callback waiting on an off-thread result.
///
/// It owns the reply the registry handed to whoever asked, so a request
/// so a request no longer has to be matched by an id afterwards — and the wait
/// dies with the config that issued it, instead of leaving a stale entry behind
/// for a result that later arrives to find its callback gone.
pub trait PendingScriptResult: Send {
    /// Delivers the result if it has arrived, consuming the callback.
    fn try_deliver(&mut self) -> Option<Result<(), ScriptError>>;
}
