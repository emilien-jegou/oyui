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
    let outcome = host.load(&path, BoxedHandler::empty(), None);
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
fn unknown_modes_are_reported_and_ignored() {
    let source = r#"
        pub fn config() {
            on_mode("nope", || { keybind("ctrl-bad", || view::file::cursor::up(1)); });
        }
    "#;
    let (_host, reg, error) = load(source);

    let error = error.expect("unknown mode must surface an error");
    assert!(
        error.message.contains("unknown view 'nope'"),
        "unexpected message: {}",
        error.message
    );
    assert!(callback_id(&reg, KeybindMode::View(View::File), "ctrl-bad").is_none());
    assert!(callback_id(&reg, KeybindMode::Global, "ctrl-bad").is_none());
}

#[test]
fn multi_modifier_keybinds_are_bound() {
    let source = r#"
        pub fn config() {
            keybind("ctrl-shift-j", || view::file::cursor::down(1));
        }
    "#;
    let (_host, reg, error) = load(source);

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    callback_id(&reg, KeybindMode::Global, "ctrl-shift-j")
        .expect("ctrl-shift-j must be bound as one chord");
}

#[test]
fn invalid_keybind_names_are_reported() {
    let source = r#"
        pub fn config() {
            keybind("bogus", || ());
        }
    "#;
    let (_host, _reg, error) = load(source);

    let error = error.expect("invalid keybind must surface an error");
    assert!(
        error.message.contains("invalid keybind 'bogus'"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn unbind_removes_a_default_binding() {
    let source = r#"
        pub fn config() {
            unbind("s");
        }
    "#;
    let (_host, reg, error) = load(source);

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    let target = Keybind::parse("s");
    assert!(
        !reg.bindings.iter().any(|(_, k, _)| k.binds(&target)),
        "unbind('s') must drop the default split binding"
    );
}

#[test]
fn unbind_all_clears_core_bindings() {
    let source = r#"
        pub fn config() { unbind_all(); }
    "#;
    let (_host, reg, error) = load(source);

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    assert!(
        reg.bindings.is_empty(),
        "unbind_all must remove every core binding"
    );
}

#[test]
fn reset_keybinds_restores_defaults() {
    let source = r#"
        pub fn config() { unbind_all(); reset_keybinds(); }
    "#;
    let (_host, reg, error) = load(source);

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    assert_eq!(reg.bindings.len(), default_keybinds().bindings.len());
}

#[test]
fn keybinds_can_be_added_after_unbind_all() {
    let source = r#"
        pub fn config() { unbind_all(); keybind("ctrl-x", || ()); }
    "#;
    let (_host, reg, error) = load(source);

    assert!(error.is_none(), "{:?}", error.map(|e| e.message));
    callback_id(&reg, KeybindMode::Global, "ctrl-x").expect("bound after unbind_all");
    assert_eq!(reg.bindings.len(), 1);
}

#[test]
fn named_keybinds_carry_their_label() {
    let source = r#"
        pub fn config() {
            keybind_named("ctrl-y", "my custom action", || ());
        }
    "#;
    let (_host, reg, error) = load(source);
    assert!(error.is_none(), "{:?}", error.map(|e| e.message));

    let entry = reg
        .entries()
        .into_iter()
        .find(|e| e.keys.contains(&"ctrl-y".to_string()))
        .expect("ctrl-y must be bound");
    assert_eq!(entry.labels, vec!["my custom action".to_string()]);
}

#[test]
fn commands_and_events_round_trip() {
    let source = r#"
        pub fn config() {
            command("go-down", |args| view::file::cursor::down(5));
            on("file_opened", || ());
        }
    "#;
    let (host, _reg, error) = load(source);
    assert!(error.is_none(), "{:?}", error.map(|e| e.message));

    host.call_command("go-down", "").expect("command runs");
    host.call_event("file_opened").expect("event runs");
    assert!(host.call_command("missing", "").is_err());
}

/// Every bundled example must compile against the live API. Keeps
/// `examples/*.rn` honest as the surface evolves.
#[test]
fn bundled_examples_compile() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;

    for entry in fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.extension() != Some(OsStr::new("rn")) {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read example");
        let (_host, _reg, error) = load(&source);
        assert!(
            error.is_none(),
            "{} failed to load: {:?}",
            path.display(),
            error.map(|e| e.message)
        );
        checked += 1;
    }

    assert!(checked >= 10, "expected 10+ examples, checked {checked}");
}

#[test]
fn the_full_api_surface_compiles() {
    let source = r#"
        pub fn config() {
            let _ = global::left_path();
            let _ = global::right_path();
            let _ = global::base_path();
            let _ = global::view();
            let _ = global::algorithm();
            let _ = global::operation();
            let _ = global::writable();
            let _ = global::conflict_count();
            global::switch("tree");
            global::command("invert");
            global::clear_error();
            global::notify("hi");
            global::warn("careful");
            global::error("bad");
            global::clear_message();
            global::copy("text");
            global::help();
            global::resolve();
            global::undo();
            global::redo();

            let _ = analysis::files_containing("needle");
            analysis::files_containing_async("needle", |matches| {
                let _ = matches;
            });

            let _ = settings::scrolloff::get();
            settings::scrolloff::set(3);
            settings::context_lines::set(4);

            let _ = view::file::path();
            let _ = view::file::folded();
            let _ = view::file::cursor::row();
            let _ = view::file::cursor::row_count();
            if let Some(_hunk) = view::file::cursor::hunk_index() {}
            let _ = view::file::cursor::hunk_count();
            let _ = view::file::cursor::kind();
            let _ = view::file::cursor::text();
            let _ = view::file::cursor::old_line();
            let _ = view::file::cursor::new_line();
            let _ = view::file::cursor::is_staged();
            let _ = view::file::staging::state();
            view::file::staging::split_at(0, 1);
            view::file::staging::join();
            view::file::staging::set_hunk(0, true);
            view::file::staging::stage_all();
            view::file::staging::unstage_all();

            let _ = view::file::inspect::hunk_has();
            if let Some(_hunk) = view::file::inspect::hunk_index() {}
            let _ = view::file::inspect::hunk_count();
            let _ = view::file::inspect::hunk_text();
            let _ = view::file::inspect::hunk_marker();
            let _ = view::file::inspect::hunk_is_staged();
            let _ = view::file::inspect::diff_text();
            let _ = view::file::inspect::new_text();
            let _ = view::file::inspect::old_text();
            let _ = view::file::inspect::staged_text();
            let _ = view::file::inspect::stats();
            let _ = view::file::inspect::is_binary();

            let _ = view::tree::inspect::files();
            let _ = view::tree::inspect::path_at(0);
            let _ = view::tree::inspect::is_dir_at(0);
            let _ = view::tree::inspect::staging_at(0);
            let _ = view::tree::inspect::file_diff_text("a");
            let _ = view::tree::inspect::file_stats("a");
            let _ = view::tree::inspect::file_staging("a");

            let _ = view::tree::selected_index();
            let _ = view::tree::row_count();
            let _ = view::tree::selected_path();
            let _ = view::tree::selected_name();
            let _ = view::tree::selected_is_dir();
            let _ = view::tree::selected_staging();
            let _ = view::tree::is_folded("a");
            view::tree::directory::expand_all();
            view::tree::directory::collapse_all();
            view::tree::staging::set("a", true);
            view::tree::staging::set_matching("**/*.md", true);
            view::tree::staging::stage_all();
            view::tree::staging::unstage_all();

            let _ = theme::name();
            let _ = theme::list();
            let _ = theme::gradient::get();
            theme::gradient::set(true);
            theme::dimmer::set("red");

            ui::hint::set("file", "j=down k=up");
            ui::hint::clear("tree");
            let _ = ui::tree_row_format::get();
            ui::tree_row_format::set("state icon name stats");
            let _ = ui::status::get();
            ui::status::set("ready");

            keybind("ctrl-shift-j", || view::file::cursor::down(1));
            keybind_named("ctrl-y", "labelled", || ());
            unbind("s");
            unbind_all();
            reset_keybinds();
            command("go", |args| global::command("invert"));
            let _ = command_names();
            on("file_opened", || global::clear_error());
        }
    "#;
    let (_host, _reg, error) = load(source);

    assert!(
        error.is_none(),
        "every documented function must resolve: {:?}",
        error.map(|e| e.message)
    );
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
