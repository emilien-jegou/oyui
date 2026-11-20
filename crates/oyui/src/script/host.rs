//! Rune-backed [`ScriptHost`]: owns the engine and its script callbacks.
//!
//! Rune `Function` values are neither `Send` nor `Sync`, so callbacks are
//! stored here as [`SyncFunction`] and reached by the input layer only
//! through an opaque [`CallbackId`].

use super::compile;
use super::natives::Registration;
use super::{natives, CallbackId, PendingScriptResult, ScriptError, ScriptHost, ScriptLoad};
use crate::actions::BoxedHandler;
use parking_lot::Mutex;
use rune::runtime::SyncFunction;
use rune::{Context, ContextError};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, info_span};

/// Script host for the rune engine.
pub struct RuneHost {
    pub(super) callbacks: Arc<Mutex<HashMap<CallbackId, SyncFunction>>>,
    pub(super) registration: Arc<Mutex<Registration>>,
    pub(super) next_id: Arc<std::sync::atomic::AtomicU64>,
    /// Named callbacks registered with `command::register`.
    pub(super) commands: Arc<Mutex<HashMap<String, SyncFunction>>>,
    /// Zero-arg callbacks registered with `on`, keyed by event name.
    pub(super) events: Arc<Mutex<HashMap<String, Vec<Arc<SyncFunction>>>>>,
    /// Callbacks waiting on an off-thread result, each owning its reply.
    pub(super) pending: Arc<Mutex<Vec<Box<dyn PendingScriptResult>>>>,
}

impl Default for RuneHost {
    fn default() -> Self {
        Self::new()
    }
}

impl RuneHost {
    /// Creates a host with no script-defined callbacks yet.
    pub fn new() -> Self {
        Self {
            callbacks: Arc::new(Mutex::new(HashMap::new())),
            registration: Arc::new(Mutex::new(Registration::fresh())),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            commands: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Context handed to the language server: same modules, unused store.
    pub fn lsp_context(handler: BoxedHandler) -> Result<Context, ContextError> {
        natives::build_context(&Self::new(), handler, None)
    }
}

impl ScriptHost for RuneHost {
    fn load(
        &mut self,
        path: &Path,
        handler: BoxedHandler,
        worker: Option<Arc<crate::worker::EventRegistry>>,
    ) -> ScriptLoad {
        let span = info_span!("load_config", path = %path.display());
        let _enter = span.enter();

        if !path.exists() {
            info!("Config file is absent. Keeping current keybinds.");
            return ScriptLoad {
                keybinds: None,
                error: None,
            };
        }

        info!("Config file found. Preparing to compile.");
        // Compile into a fresh host and swap it in only once the script
        // compiles: clearing first meant a half-typed reload destroyed every
        // binding and callback the running config had registered. The id
        // counter is shared so callback ids stay unique across reloads even if
        // the old keybind registry is still on screen.
        let mut staging = RuneHost::new();
        staging.next_id = self.next_id.clone();

        let context = match natives::build_context(&staging, handler, worker) {
            Ok(context) => context,
            Err(e) => {
                return failed(format!("Failed to build context: {e}"));
            }
        };
        let mut vm = match compile::build_vm(path, context) {
            Ok(vm) => vm,
            Err(e) => return failed(e.message),
        };

        let run_error = compile::run_config_script(&mut vm).err().map(|e| e.message);
        let keybinds = staging.registration.lock().registry.clone();
        let native_error = staging.registration.lock().error.take();
        info!("Config loaded successfully");

        // The script ran, so its registrations are the live ones. A `config()`
        // failure aborts the script, so it wins over native diagnostics
        // collected before the abort; otherwise surface the native.
        let error = run_error.or(native_error);
        // Retire the old registrations before adopting the new host. `pending`
        // must be cleared explicitly: each parked callback holds a clone of the
        // queue's `Arc`, so merely dropping the host's handle would leave the
        // cycle alive and its reply slot reserved forever.
        self.callbacks.lock().clear();
        self.commands.lock().clear();
        self.events.lock().clear();
        self.pending.lock().clear();
        *self = staging;
        ScriptLoad {
            keybinds: Some(keybinds),
            error: error.map(ScriptError::new),
        }
    }

    fn call(&self, id: CallbackId) -> Result<(), ScriptError> {
        let callbacks = self.callbacks.lock();
        let Some(cb) = callbacks.get(&id) else {
            return Err(ScriptError::new(format!(
                "script callback {id:?} is no longer registered"
            )));
        };
        cb.call::<()>(())
            .into_result()
            .map_err(|e| ScriptError::new(e.to_string()))
    }

    fn call_command(&self, name: &str, args: &str) -> Result<(), ScriptError> {
        let commands = self.commands.lock();
        let Some(cb) = commands.get(name) else {
            return Err(ScriptError::new(format!("unknown command '{name}'")));
        };
        cb.call::<()>((args.to_string(),))
            .into_result()
            .map_err(|e| ScriptError::new(format!("command '{name}' failed: {e}")))
    }

    fn call_event(&self, event: &str) -> Result<(), ScriptError> {
        let callbacks: Vec<Arc<SyncFunction>> =
            self.events.lock().get(event).cloned().unwrap_or_default();
        for cb in callbacks {
            cb.call::<()>(())
                .into_result()
                .map_err(|e| ScriptError::new(format!("event '{event}' failed: {e}")))?;
        }
        Ok(())
    }

    fn park(&self, task: Box<dyn PendingScriptResult>) {
        self.pending.lock().push(task);
    }

    fn drain_pending(&self) -> Vec<ScriptError> {
        let mut pending = self.pending.lock();
        let mut failed = Vec::new();

        // `retain_mut` keeps whatever is still running; everything delivered or
        // failed is consumed here.
        pending.retain_mut(|task| match task.try_deliver() {
            Some(Err(e)) => {
                failed.push(e);
                false
            }
            Some(Ok(())) => false,
            None => true,
        });

        failed
    }
}

/// Builds a failed load, keeping `keybinds` so partial bindings survive.
fn failed(message: String) -> ScriptLoad {
    debug!(error = %message, "Config script failed");
    ScriptLoad {
        // `None` keeps the keybinds already in use: a compile error must not
        // downgrade a working config to the built-in defaults.
        keybinds: None,
        error: Some(ScriptError::new(message)),
    }
}
