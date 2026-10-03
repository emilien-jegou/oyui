//! Tests for the script port: round-trips through [`ScriptHost`] and the
//! invariant that no other module names the engine.

use super::*;
use crate::actions::keybinds::{default_keybinds, KeySource, KeybindMode, View};
use crate::actions::ActionTarget;
use crate::commons::input::Keybind;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes fixtures written by concurrently running tests.
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Runs `source` through a fresh host, returning it with the keybinds and error.
fn load(source: &str) -> (RuneHost, KeybindRegistry, Option<ScriptError>) {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("oyui_script_{}_{n}.rn", std::process::id()));
    fs::write(&path, source).expect("write fixture");
    let mut host = RuneHost::new();
    let outcome = host.load(&path, BoxedHandler::empty());
    let _ = fs::remove_file(&path);
    (
        host,
        outcome.keybinds.expect("load always reports keybinds"),
        outcome.error,
    )
}

/// The single script callback bound to `kb` in `mode`, if any.
fn callback_id(reg: &KeybindRegistry, mode: KeybindMode, kb: &str) -> Option<CallbackId> {
    let kb = KeySource::Keybind(Keybind::parse(kb));
    reg.bindings
        .iter()
        .find_map(|(m, k, t)| match (m, k, t.as_slice()) {
            (m, k, [ActionTarget::Dynamic(id)]) if *m == mode && *k == kb => Some(*id),
            _ => None,
        })
}

#[test]
fn script_keybinds_arrive_as_opaque_handles_and_still_run() {
    let source = r#"
        pub fn config() {
            theme::file_staged_highlight::set(LineHighlightMode::Gradient(0.05));
            keybind("ctrl-j", || view::file::cursor::down(5));
            keybind("ctrl-h", || ());
            on_mode("file", || { keybind("ctrl-x", || view::file::cursor::up(1)); });
        }
    "#;
    let (host, reg, error) = load(source);

    assert!(
        error.is_none(),
        "load failed: {:?}",
        error.map(|e| e.message)
    );
    callback_id(&reg, KeybindMode::Global, "ctrl-j").expect("ctrl-j bound");
    callback_id(&reg, KeybindMode::View(View::File), "ctrl-x").expect("ctrl-x bound");

    // The handle kept in the registry must still reach a live callback.
    let id = callback_id(&reg, KeybindMode::Global, "ctrl-h").expect("ctrl-h bound");
    host.call(id).expect("callback still registered");
}

#[test]
fn modes_that_the_app_does_not_know_are_ignored() {
    let source = r#"
        pub fn config() {
            on_mode("nope", || { keybind("ctrl-bad", || view::file::cursor::up(1)); });
        }
    "#;
    let (_host, reg, error) = load(source);

    assert!(
        error.is_none(),
        "load failed: {:?}",
        error.map(|e| e.message)
    );
    assert!(callback_id(&reg, KeybindMode::View(View::File), "ctrl-bad").is_none());
    assert!(callback_id(&reg, KeybindMode::Global, "ctrl-bad").is_none());
}

#[test]
fn a_script_without_an_entrypoint_is_not_an_error() {
    let (_host, reg, error) = load("fn helper() {}");

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    assert_eq!(reg.bindings.len(), default_keybinds().bindings.len());
}

#[test]
fn a_broken_script_reports_and_falls_back_to_defaults() {
    let (_host, reg, error) = load("pub fn config() { let = }");

    assert!(error.is_some(), "expected a compile error, got success");
    assert_eq!(reg.bindings.len(), default_keybinds().bindings.len());
}

#[test]
fn the_line_highlight_bridge_round_trips() {
    use crate::config::LineHighlightMode;
    use oyui_rune_actions::ScriptRepr;

    for mode in [
        LineHighlightMode::None,
        LineHighlightMode::Solid,
        LineHighlightMode::Gradient(0.05),
    ] {
        let back = LineHighlightMode::from_repr(mode.into_repr());
        assert_eq!(format!("{mode:?}"), format!("{back:?}"));
    }
}

#[test]
fn rune_stays_behind_the_script_boundary() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut leaked = Vec::new();
    collect_rune_uses(&src, &src, &mut leaked);

    assert!(
        leaked.is_empty(),
        "engine references outside src/script: {leaked:?}"
    );
}

/// Walks `dir`, recording every file under `src` that names the engine.
fn collect_rune_uses(root: &Path, dir: &Path, leaked: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read src") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rune_uses(root, &path, leaked);
            continue;
        }
        if path.extension() != Some(OsStr::new("rs")) {
            continue;
        }
        let rel = path.strip_prefix(root).expect("path under src");
        if rel.starts_with("script") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source");
        let names_engine =
            text.contains("rune::") || text.lines().any(|l| l.trim_start().starts_with("use rune"));
        if names_engine {
            leaked.push(rel.display().to_string());
        }
    }
}
