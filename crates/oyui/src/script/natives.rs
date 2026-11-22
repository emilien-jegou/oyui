//! Registration natives for script keybindings, commands, events, and analysis.

use super::host::RuneHost;
use super::{CallbackId, PendingScriptResult, ScriptError};
use crate::actions::keybinds::{default_keybinds, KeybindMode, KeybindRegistry, View};
use crate::commons::input::Keybind;
use crate::worker::tasks::analysis::AnalysisRes;
use oyui_tasker::worker::Reply as ReplyHandle;
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
    /// First native-side registration failure; surfaced by load.
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

/// Stores `cb` and binds it to `kb` under the active registration scope.
fn register_keybind(
    kb: &str,
    label: Option<String>,
    cb: Function,
    callbacks: &Mutex<HashMap<CallbackId, SyncFunction>>,
    next_id: &AtomicU64,
    registration: &Mutex<Registration>,
    forced_mode: Option<KeybindMode>,
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
    let mode = forced_mode.or_else(|| state.mode.clone());
    state.registry = match mode {
        Some(mode) => registry.register_fn_mode(mode, kb, id),
        None => registry.register_fn(kb, id),
    };
    if let Some(label) = label {
        state.registry.label_callback(id, label);
    }
}

/// Runs `cb` while registrations are scoped to the selected view.
fn register_in_view(mode: &str, cb: Function, registration: &Mutex<Registration>) {
    let mode = match mode.to_lowercase().as_str() {
        "file" => KeybindMode::View(View::File),
        "tree" => KeybindMode::View(View::Tree),
        other => {
            let message =
                format!("keybind::in_view: unknown view '{other}' (expected 'file' or 'tree')");
            error!("{message}");
            registration.lock().error.get_or_insert(message);
            return;
        }
    };

    registration.lock().mode = Some(mode);
    let result = cb.call::<()>(());
    registration.lock().mode = None;

    if let Err(e) = result.into_result() {
        let message = format!("keybind::in_view callback failed: {e}");
        error!("{message}");
        registration.lock().error.get_or_insert(message);
    }
}

/// Removes a binding from the requested scope.
fn remove_binding(kb: &str, mode: Option<KeybindMode>, registration: &Mutex<Registration>) {
    match Keybind::parse_checked(kb) {
        Ok(kb) => {
            let mut state = registration.lock();
            match mode {
                Some(mode) => state.registry.remove_in_mode(mode, &kb),
                None => state.registry.remove(&kb),
            }
        }
        Err(message) => {
            error!("{message}");
            registration.lock().error.get_or_insert(message);
        }
    }
}

/// Stores `cb` as a registered command the `:` palette can run.
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

/// Stores `cb` to run whenever a known application event is emitted.
fn register_event(
    event: &str,
    cb: Function,
    events: &Mutex<HashMap<String, Vec<Arc<SyncFunction>>>>,
    registration: &Mutex<Registration>,
) {
    const KNOWN_EVENTS: &[&str] = &["config_reload", "file_opened", "view_changed", "quit"];
    if !KNOWN_EVENTS.contains(&event) {
        let message = format!(
            "unknown event '{event}' (expected one of: {})",
            KNOWN_EVENTS.join(", ")
        );
        error!("{message}");
        registration.lock().error.get_or_insert(message);
        return;
    }
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

    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);
    let mut m = Module::with_item(["keybind"])?;
    m.function("global", move |kb: String, cb: Function| {
        register_keybind(
            &kb,
            None,
            cb,
            &callbacks,
            &next_id,
            &registration,
            Some(KeybindMode::Global),
        );
    })
    .build()?;
    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);
    m.function("bind", move |kb: String, cb: Function| {
        register_keybind(&kb, None, cb, &callbacks, &next_id, &registration, None);
    })
    .build()?;
    let callbacks = Arc::clone(&host.callbacks);
    let next_id = Arc::clone(&host.next_id);
    let registration = Arc::clone(&host.registration);
    m.function("named", move |kb: String, label: String, cb: Function| {
        register_keybind(
            &kb,
            Some(label),
            cb,
            &callbacks,
            &next_id,
            &registration,
            None,
        );
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("in_view", move |view: String, cb: Function| {
        register_in_view(&view, cb, &registration)
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("remove_global", move |kb: String| {
        remove_binding(&kb, Some(KeybindMode::Global), &registration)
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("remove_from_view", move |view: String, kb: String| {
        let mode = match view.to_lowercase().as_str() {
            "file" => Some(KeybindMode::View(View::File)),
            "tree" => Some(KeybindMode::View(View::Tree)),
            _ => None,
        };
        if let Some(mode) = mode {
            remove_binding(&kb, Some(mode), &registration);
        } else {
            let message = format!("keybind::remove_from_view: unknown view '{view}'");
            registration.lock().error.get_or_insert(message);
        }
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("remove_everywhere", move |kb: String| {
        remove_binding(&kb, None, &registration)
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("remove_all", move || registration.lock().registry.clear())
        .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("restore_defaults", move || {
        registration.lock().registry.reset()
    })
    .build()?;
    let registration = Arc::clone(&host.registration);
    m.function("list", move || -> Vec<String> {
        registration
            .lock()
            .registry
            .entries()
            .into_iter()
            .map(|entry| {
                let scope = match entry.mode {
                    KeybindMode::Global => "global",
                    KeybindMode::View(View::File) => "file",
                    KeybindMode::View(View::Tree) => "tree",
                };
                format!(
                    "{scope}: {} = {}",
                    entry.keys.join(", "),
                    entry.labels.join(", ")
                )
            })
            .collect()
    })
    .build()?;
    context.install(m)?;

    let commands = Arc::clone(&host.commands);
    let registration = Arc::clone(&host.registration);
    let mut m = Module::with_item(["commands"])?;
    m.function("register", move |name: String, cb: Function| {
        register_command(&name, cb, &commands, &registration);
    })
    .build()?;
    let commands = Arc::clone(&host.commands);
    m.function("list", move || -> Vec<String> {
        let mut names: Vec<String> = commands.lock().keys().cloned().collect();
        names.sort_unstable();
        names
    })
    .build()?;
    context.install(m)?;

    let events = Arc::clone(&host.events);
    let registration = Arc::clone(&host.registration);
    let mut m = Module::with_item(["events"])?;
    m.function("on", move |event: String, cb: Function| {
        register_event(&event, cb, &events, &registration);
    })
    .build()?;
    context.install(m)?;

    // Off-thread analysis native; always registered so the LSP can see it,
    // but a no-op when there is no worker.
    let pending = Arc::clone(&host.pending);
    let registration = Arc::clone(&host.registration);
    let mut m = Module::with_item(["analysis"])?;
    m.function(
        "find_files_containing_async",
        move |pattern: String, cb: Function| {
            let Some(worker) = &worker else {
                error!("analysis::find_files_containing_async requires a running worker");
                return;
            };
            let Ok(cb) = cb.into_sync() else {
                let message =
                    "analysis callback captures a non-constant value; dropped".to_string();
                error!("{message}");
                registration.lock().error.get_or_insert(message);
                return;
            };
            match worker.ask(crate::worker::tasks::analysis::AnalysisReq { pattern }) {
                Ok(reply) => pending.lock().push(Box::new(PendingAnalysis { reply, cb })),
                Err(e) => {
                    let message = format!("analysis request failed: {e}");
                    error!("{message}");
                    registration.lock().error.get_or_insert(message);
                }
            }
        },
    )
    .build()?;
    context.install(m)?;

    crate::actions::register_actions(&mut context, handler)?;
    Ok(context)
}

/// A script callback waiting on the analysis result it asked for.
///
/// It owns the reply, so the answer is delivered to exactly this callback and
/// the request's state disappears with it — no id to match and no entry left
/// behind for a result that arrives after a reload.
struct PendingAnalysis {
    reply: ReplyHandle<AnalysisRes>,
    cb: SyncFunction,
}

impl PendingScriptResult for PendingAnalysis {
    fn try_deliver(&mut self) -> Option<Result<(), ScriptError>> {
        let answer = self.reply.try_recv()?;
        Some(
            self.cb
                .call::<()>((answer.matches, answer.error))
                .into_result()
                .map_err(|e| ScriptError::new(format!("analysis callback failed: {e}"))),
        )
    }
}
