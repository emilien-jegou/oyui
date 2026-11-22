//! Rune-backed [`ScriptHost`]: owns the engine and its script callbacks.
//!
//! Rune `Function` values are neither `Send` nor `Sync`, so callbacks are
//! stored here as [`SyncFunction`] and reached by the input layer only
//! through an opaque [`CallbackId`].

use super::compile;
use super::natives::Registration;
use super::{
    natives, CallbackId, PendingScriptResult, ScriptContext, ScriptError, ScriptHost, ScriptLoad,
};
use crate::actions::BoxedHandler;
use parking_lot::Mutex;
use rune::runtime::SyncFunction;
use rune::{Context, ContextError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// A finished off-thread reload, waiting for [`Self::apply_parked`].
    pending_reload: Arc<Mutex<Option<ParkedReload>>>,
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
            pending_reload: Arc::new(Mutex::new(None)),
        }
    }

    /// Kicks off an off-thread reload of `path`.
    ///
    /// The compile runs on the blocking pool and lands in the parked-reload
    /// shelf; [`Config`]'s per-frame drain applies it (finding 4.1: the Rune
    /// compile no longer blocks the render loop). The next-id counter is
    /// shared so callback ids stay unique even across in-flight reloads; a
    /// reload finishing while another sits undrained replaces it — newest
    /// wins, same as the event coalescing. Must be called from the tokio
    /// runtime; the only caller is the UI loop's worker-event handling.
    pub fn reload(&self, path: PathBuf, ctx: &ScriptContext) {
        info!("Reloading config off thread...");
        let next_id = self.next_id.clone();
        let handler = ctx.handler.clone();
        let worker = ctx.worker.clone();
        let shelf = self.pending_reload.clone();
        tokio::task::spawn_blocking(move || {
            let parked = compile_staging(path, next_id, handler, worker);
            *shelf.lock() = Some(parked);
        });
    }

    /// Adopts a finished reload, if one landed since the last frame.
    ///
    /// Returns `(keybinds, error)` for the caller to install/report. A
    /// staged host is only adopted when the script *ran*: a script that
    /// failed to execute must not half-replace a working config (the same
    /// rule the compile path follows).
    pub fn apply_parked(
        &mut self,
    ) -> (
        Option<crate::actions::keybinds::KeybindRegistry>,
        Option<ScriptError>,
    ) {
        let Some(parked) = self.pending_reload.lock().take() else {
            return (None, None);
        };
        if let Some(staging) = parked.host {
            // Retire the old registrations before adopting the new host.
            // `pending` must be cleared explicitly: each parked callback
            // holds a clone of the queue's `Arc`, so merely dropping the
            // host's handle would leave the cycle alive and its reply slot
            // reserved forever.
            self.callbacks.lock().clear();
            self.commands.lock().clear();
            self.events.lock().clear();
            self.pending.lock().clear();
            *self = staging;
        }
        (parked.keybinds, parked.error)
    }

    /// Delivers every async script result that has arrived; returns what
    /// went wrong. Host plumbing, not part of the [`ScriptHost`] port — the
    /// config's per-frame drain walks this seam.
    pub fn drain_pending(&self) -> Vec<ScriptError> {
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

    /// True when an off-thread reload has finished and is waiting to be
    /// applied by [`Self::apply_parked`].
    pub fn has_parked_reload(&self) -> bool {
        self.pending_reload.lock().is_some()
    }

    /// Context handed to the language server: same modules, unused store.
    pub fn lsp_context(handler: BoxedHandler) -> Result<Context, ContextError> {
        natives::build_context(&Self::new(), handler, None)
    }
}

/// A finished compile of one config revision, parked for the UI thread.
pub(super) struct ParkedReload {
    /// The staged host to adopt; `None` keeps the current one — the script
    /// compiled but failed to run, so its half-registrations are discarded.
    host: Option<RuneHost>,
    keybinds: Option<crate::actions::keybinds::KeybindRegistry>,
    error: Option<ScriptError>,
}

/// Compiles `path` into a fresh staging host (the reload body, run on the
/// blocking pool).
fn compile_staging(
    path: PathBuf,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    handler: BoxedHandler,
    worker: Option<Arc<crate::worker::EventRegistry>>,
) -> ParkedReload {
    // The watcher's first pass may legitimately run on a config that does
    // not exist yet (it was seeded before the file appeared): absent means
    // "nothing to do", not an error.
    if !path.exists() {
        debug!("Config file absent; keeping current config");
        return ParkedReload {
            host: None,
            keybinds: None,
            error: None,
        };
    }

    let span = info_span!("reload_config", path = %path.display());
    let _enter = span.enter();

    info!("Config file found. Preparing to compile.");
    let mut staging = RuneHost::new();
    staging.next_id = next_id;

    let context = match natives::build_context(&staging, handler, worker) {
        Ok(context) => context,
        Err(e) => {
            return ParkedReload {
                host: None,
                keybinds: None,
                error: Some(ScriptError::new(format!("Failed to build context: {e}"))),
            }
        }
    };
    let mut vm = match compile::build_vm(&path, context) {
        Ok(vm) => vm,
        Err(e) => {
            return ParkedReload {
                host: None,
                keybinds: None,
                error: Some(e),
            }
        }
    };

    // The script ran, so its registrations are adoptable. A `config()`
    // failure aborts the script: its half-done registrations are discarded
    // and the previous config stays live (see `apply_parked`).
    let run_error = compile::run_config_script(&mut vm).err();
    if let Some(e) = run_error {
        return ParkedReload {
            host: None,
            keybinds: None,
            error: Some(e),
        };
    }
    let keybinds = staging.registration.lock().registry.clone();
    let native_error = staging.registration.lock().error.take();
    if let Some(message) = native_error {
        return ParkedReload {
            host: None,
            keybinds: None,
            error: Some(ScriptError::new(message)),
        };
    }
    info!("Config loaded successfully");
    ParkedReload {
        host: Some(staging),
        keybinds: Some(keybinds),
        error: None,
    }
}

impl ScriptHost for RuneHost {
    /// The synchronous bootstrap load: compiles into a fresh host and swaps
    /// it in only once the script both compiles and runs. Reloads use
    /// [`RuneHost::reload`] + [`RuneHost::apply_parked`] instead.
    fn load(&mut self, path: &Path, ctx: &ScriptContext) -> ScriptLoad {
        if !path.exists() {
            info!("Config file is absent. Keeping current keybinds.");
            return ScriptLoad {
                keybinds: None,
                error: None,
            };
        }

        let span = info_span!("load_config", path = %path.display());
        let _enter = span.enter();

        info!("Config file found. Preparing to compile.");
        // The id counter is shared so callback ids stay unique across
        // reloads even if the old keybind registry is still on screen.
        let staged = compile_staging(
            path.to_path_buf(),
            self.next_id.clone(),
            ctx.handler.clone(),
            ctx.worker.clone(),
        );
        match staged.host {
            Some(staging) => {
                self.callbacks.lock().clear();
                self.commands.lock().clear();
                self.events.lock().clear();
                self.pending.lock().clear();
                *self = staging;
            }
            None => {}
        }
        ScriptLoad {
            keybinds: staged.keybinds,
            error: staged.error,
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
}
