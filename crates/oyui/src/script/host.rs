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
        }
    }

    /// Context handed to the language server: same modules, unused store.
    pub fn lsp_context(handler: BoxedHandler) -> Result<Context, ContextError> {
        natives::build_context(&Self::new(), handler)
    }
}

impl ScriptHost for RuneHost {
    fn load(&mut self, path: &Path, handler: BoxedHandler) -> ScriptLoad {
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
        *self.registration.lock() = Registration::fresh();

        let context = match natives::build_context(self, handler) {
            Ok(context) => context,
            Err(e) => {
                return failed(default_keybinds(), format!("Failed to build context: {e}"));
            }
        };
        let mut vm = match compile::build_vm(path, context) {
            Ok(vm) => vm,
            Err(e) => return failed(default_keybinds(), e.message),
        };

        let error = compile::run_config_script(&mut vm).err().map(|e| e.message);
        let keybinds = self.registration.lock().registry.clone();
        info!("Config loaded successfully");

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
}

/// Builds a failed load, keeping `keybinds` so partial bindings survive.
fn failed(keybinds: KeybindRegistry, message: String) -> ScriptLoad {
    debug!(error = %message, "Config script failed");
    ScriptLoad {
        keybinds: Some(keybinds),
        error: Some(ScriptError::new(message)),
    }
}
