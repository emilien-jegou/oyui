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
    /// First native-side failure (bad keybind / `on_mode` panic); surfaced by load.
    pub(super) error: Option<String>,
}

impl Registration {
    pub(super) fn fresh() -> Self {
        Self {
            registry: default_keybinds(),
            mode: None,
            error: None,
        }
    }
}

/// Stores `cb` and binds it to `kb` under the mode currently being wired.
fn register_keybind(
    kb: &str,
    label: Option<String>,
    cb: Function,
    callbacks: &Mutex<HashMap<CallbackId, SyncFunction>>,
    next_id: &AtomicU64,
    registration: &Mutex<Registration>,
) {
    let cb = match cb.into_sync() {
        Ok(cb) => cb,
        Err(e) => {
            let message = format!("keybind '{kb}' captures a non-constant value ({e}); dropped");
            error!("{message}");
            let mut state = registration.lock();
            state.error.get_or_insert(message);
            return;
        }
    };

    let kb = match Keybind::parse_checked(kb) {
        Ok(kb) => kb,
        Err(message) => {
            error!("{message}");
            registration.lock().error.get_or_insert(message);
            return;
        }
    };
    let id = CallbackId(next_id.fetch_add(1, Ordering::Relaxed));
    callbacks.lock().insert(id, cb);

    let mut state = registration.lock();
    let registry = std::mem::take(&mut state.registry);
    let mode = state.mode.clone();
    state.registry = match mode {
        Some(mode) => registry.register_fn_mode(mode, kb, id),
        None => registry.register_fn(kb, id),
    };
    if let Some(label) = label {
        state.registry.label_callback(id, label);
    }
}

/// Runs `cb` with `mode` active so nested keybinds land in that scope.
fn run_scoped(mode: &str, cb: Function, registration: &Mutex<Registration>) {
    let mode = match mode.to_lowercase().as_str() {
        "file" => KeybindMode::View(View::File),
        "tree" => KeybindMode::View(View::Tree),
        other => {
            let message = format!("on_mode: unknown view '{other}' (expected 'file' or 'tree')");
            error!("{message}");
            registration.lock().error.get_or_insert(message);
            return;
        }
    };

    registration.lock().mode = Some(mode);
    let result = cb.call::<()>(());
    registration.lock().mode = None;

    if let Err(e) = result.into_result() {
        let message = format!("on_mode callback failed: {e}");
        error!("{message}");
        registration.lock().error.get_or_insert(message);
    }
}

/// Removes every binding for `kb` from the registry being built.
fn unbind(kb: &str, registration: &Mutex<Registration>) {
    match Keybind::parse_checked(kb) {
        Ok(kb) => registration.lock().registry.remove(&kb),
        Err(message) => {
            error!("{message}");
            registration.lock().error.get_or_insert(message);
        }
    }
}

/// Stores `cb` as a named command the `:` palette can run.
fn register_command(
    name: &str,
    cb: Function,
    commands: &Mutex<HashMap<String, SyncFunction>>,
    registration: &Mutex<Registration>,
) {
    match cb.into_sync() {
        Ok(cb) => {
            commands.lock().insert(name.to_string(), cb);
        }
        Err(e) => {
            let message = format!("command '{name}' captures a non-constant value ({e}); dropped");
            error!("{message}");
            registration.lock().error.get_or_insert(message);
        }
    }
}

/// Stores `cb` to run whenever `event` is emitted.
fn register_event(
    event: &str,
    cb: Function,
    events: &Mutex<HashMap<String, Vec<Arc<SyncFunction>>>>,
    registration: &Mutex<Registration>,
) {
    match cb.into_sync() {
        Ok(cb) => {
            events
                .lock()
                .entry(event.to_string())
                .or_default()
                .push(Arc::new(cb));
        }
        Err(e) => {
            let message = format!("on('{event}') captures a non-constant value ({e}); dropped");
            error!("{message}");
            registration.lock().error.get_or_insert(message);
        }
    }
}

/// Builds the script context; the natives write into `host`'s stores.
pub(super) fn build_context(
    host: &RuneHost,
    handler: crate::actions::BoxedHandler,
    worker: Option<Arc<crate::worker::EventRegistry>>,
) -> Result<Context, ContextError> {
    let mut context = Context::with_default_modules()?;
    context.install(super::highlight::base_module()?)?;

    let mut m = Module::new();

    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);
    m.function("keybind", move |kb: String, cb: Function| {
        register_keybind(&kb, None, cb, &callbacks, &next_id, &registration);
    })
    .build()?;

    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);
    m.function(
        "keybind_named",
        move |kb: String, label: String, cb: Function| {
            register_keybind(&kb, Some(label), cb, &callbacks, &next_id, &registration);
        },
    )
    .build()?;

    let registration = Arc::clone(&host.registration);
    m.function("on_mode", move |mode: String, cb: Function| {
        run_scoped(&mode, cb, &registration)
    })
    .build()?;

    let registration = Arc::clone(&host.registration);
    m.function("unbind", move |kb: String| {
        unbind(&kb, &registration);
    })
    .build()?;

    let registration = Arc::clone(&host.registration);
    m.function("unbind_all", move || {
        registration.lock().registry.clear();
    })
    .build()?;

    let registration = Arc::clone(&host.registration);
    m.function("reset_keybinds", move || {
        registration.lock().registry.reset();
    })
    .build()?;

    let commands = Arc::clone(&host.commands);
    let registration = Arc::clone(&host.registration);
    m.function("command", move |name: String, cb: Function| {
        register_command(&name, cb, &commands, &registration);
    })
    .build()?;

    let events = Arc::clone(&host.events);
    let registration = Arc::clone(&host.registration);
    m.function("on", move |event: String, cb: Function| {
        register_event(&event, cb, &events, &registration);
    })
    .build()?;

    // Discoverability: names of every script-registered `command`.
    let commands = Arc::clone(&host.commands);
    m.function("command_names", move || -> String {
        let mut names: Vec<String> = commands.lock().keys().cloned().collect();
        names.sort_unstable();
        names.join("\n")
    })
    .build()?;

    context.install(m)?;

    // Off-thread analysis native; always registered so the LSP can see it,
    // but a no-op when there is no worker.
    let tasks = Arc::clone(&host.tasks);
    let next_task = Arc::clone(&host.next_task);
    let mut m = Module::with_item(["analysis"])?;
    m.function(
        "files_containing_async",
        move |pattern: String, cb: Function| {
            let Some(worker) = &worker else {
                error!("analysis::files_containing_async requires a running worker");
                return;
            };
            let Ok(cb) = cb.into_sync() else {
                error!("analysis callback captures a non-constant value; dropped");
                return;
            };
            let task_id = next_task.fetch_add(1, Ordering::Relaxed);
            tasks.lock().insert(task_id, cb);
            let _ = worker.send(crate::worker::tasks::analysis::AnalysisReq { task_id, pattern });
        },
    )
    .build()?;
    context.install(m)?;

    crate::actions::register_actions(&mut context, handler)?;
    Ok(context)
}
