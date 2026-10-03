//! Script natives: the `keybind` and `on_mode` functions exposed to scripts.

use super::host::RuneHost;
use super::CallbackId;
use crate::actions::keybinds::{default_keybinds, KeybindMode, KeybindRegistry, View};
use crate::commons::input::Keybind;
use parking_lot::Mutex;
use rune::runtime::{Function, SyncFunction};
use rune::{Context, ContextError, Module};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::error;

/// Registry being built plus the mode the script is currently wiring.
pub(super) struct Registration {
    pub(super) registry: KeybindRegistry,
    pub(super) mode: Option<KeybindMode>,
}

impl Registration {
    pub(super) fn fresh() -> Self {
        Self {
            registry: default_keybinds(),
            mode: None,
        }
    }
}

/// Stores `cb` and binds it to `kb` under the mode currently being wired.
fn register_keybind(
    kb: &str,
    cb: Function,
    callbacks: &Mutex<HashMap<CallbackId, SyncFunction>>,
    next_id: &AtomicU64,
    registration: &Mutex<Registration>,
) {
    let cb = match cb.into_sync() {
        Ok(cb) => cb,
        Err(e) => {
            error!("Keybind '{kb}' captures a non-constant value ({e}); dropped");
            return;
        }
    };

    let kb = Keybind::parse(kb);
    let id = CallbackId(next_id.fetch_add(1, Ordering::Relaxed));
    callbacks.lock().insert(id, cb);

    let mut state = registration.lock();
    let registry = std::mem::take(&mut state.registry);
    let mode = state.mode.clone();
    state.registry = match mode {
        Some(mode) => registry.register_fn_mode(mode, kb, id),
        None => registry.register_fn(kb, id),
    };
}

/// Runs `cb` with `mode` active so nested keybinds land in that scope.
fn run_scoped(mode: &str, cb: Function, registration: &Mutex<Registration>) {
    let mode = match mode.to_lowercase().as_str() {
        "file" => KeybindMode::View(View::File),
        "tree" => KeybindMode::View(View::Tree),
        _ => return,
    };

    registration.lock().mode = Some(mode);
    let result = cb.call::<()>(());
    registration.lock().mode = None;

    if let Err(e) = result.into_result() {
        error!("on_mode callback failed: {e}");
    }
}

/// Builds the script context; the natives write into `host`'s stores.
pub(super) fn build_context(
    host: &RuneHost,
    handler: crate::actions::BoxedHandler,
) -> Result<Context, ContextError> {
    let mut context = Context::with_default_modules()?;
    context.install(super::highlight::base_module()?)?;

    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);

    let mut m = Module::new();
    m.function("keybind", move |kb: String, cb: Function| {
        register_keybind(&kb, cb, &callbacks, &next_id, &registration);
    })
    .build()?;

    let registration = Arc::clone(&host.registration);
    m.function("on_mode", move |mode: String, cb: Function| {
        run_scoped(&mode, cb, &registration)
    })
    .build()?;

    context.install(m)?;
    crate::actions::register_actions(&mut context, handler)?;
    Ok(context)
}
