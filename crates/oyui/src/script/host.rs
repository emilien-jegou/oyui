//! Rune-backed [`ScriptHost`]: owns the engine and its script callbacks.
//!
//! Rune `Function` values are neither `Send` nor `Sync`, so callbacks are
//! stored here as [`SyncFunction`] and reached by the input layer only
//! through an opaque [`CallbackId`].

use super::compile;
use super::natives::Registration;
use super::{natives, CallbackId, ScriptError, ScriptHost, ScriptLoad};
use crate::actions::keybinds::{default_keybinds, KeybindRegistry};
use crate::actions::BoxedHandler;
use parking_lot::Mutex;
use rune::runtime::SyncFunction;
use rune::{Context, ContextError};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tracing::{debug, info, info_span};

/// Script host for the rune engine.
pub struct RuneHost {
    pub(super) callbacks: Arc<Mutex<HashMap<CallbackId, SyncFunction>>>,
    pub(super) registration: Arc<Mutex<Registration>>,
    pub(super) next_id: Arc<AtomicU64>,
    /// Named callbacks registered with `command::register`.
    pub(super) commands: Arc<Mutex<HashMap<String, SyncFunction>>>,
    /// Zero-arg callbacks registered with `on`, keyed by event name.
    pub(super) events: Arc<Mutex<HashMap<String, Vec<Arc<SyncFunction>>>>>,
    /// One-shot callbacks awaiting an off-thread task result.
    pub(super) tasks: Arc<Mutex<HashMap<u64, SyncFunction>>>,
    /// Allocator for task callback ids.
    pub(super) next_task: Arc<AtomicU64>,
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
            next_id: Arc::new(AtomicU64::new(0)),
            commands: Arc::new(Mutex::new(HashMap::new())),
            events: Arc::new(Mutex::new(HashMap::new())),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            next_task: Arc::new(AtomicU64::new(0)),
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
        self.callbacks.lock().clear();
        self.commands.lock().clear();
        self.events.lock().clear();
        self.tasks.lock().clear();
        *self.registration.lock() = Registration::fresh();

        let context = match natives::build_context(self, handler, worker) {
            Ok(context) => context,
            Err(e) => {
                return failed(default_keybinds(), format!("Failed to build context: {e}"));
            }
        };
        let mut vm = match compile::build_vm(path, context) {
            Ok(vm) => vm,
            Err(e) => return failed(default_keybinds(), e.message),
        };

        let run_error = compile::run_config_script(&mut vm).err().map(|e| e.message);
        let keybinds = self.registration.lock().registry.clone();
        let native_error = self.registration.lock().error.take();
        info!("Config loaded successfully");

        // A `config()` failure aborts the script, so it wins over native
        // diagnostics collected before the abort; otherwise surface the native.
        let error = run_error.or(native_error);
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

    fn call_task(&self, task_id: u64, result: String) -> Result<(), ScriptError> {
        let cb =
            self.tasks.lock().remove(&task_id).ok_or_else(|| {
                ScriptError::new(format!("task {task_id} is no longer registered"))
            })?;
        cb.call::<()>((result,))
            .into_result()
            .map_err(|e| ScriptError::new(format!("task {task_id} failed: {e}")))
    }
}

/// Builds a failed load, keeping `keybinds` so partial bindings survive.
fn failed(keybinds: KeybindRegistry, message: String) -> ScriptLoad {
    debug!(error = %message, "Config script failed");
    ScriptLoad {
        keybinds: Some(keybinds),
        error: Some(ScriptError::new(message)),
    }
}
