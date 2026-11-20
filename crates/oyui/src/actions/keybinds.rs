use crossterm::event::{KeyCode, KeyEvent};

use crate::actions::{
    Action, GlobalActions, ViewFileActions, ViewFileCursorActions, ViewFileFoldActions,
    ViewFileNavActions, ViewFileScrollActions, ViewFileStagingActions, ViewTreeActions,
    ViewTreeCursorActions, ViewTreeDirectoryActions, ViewTreeStagingActions,
};
use crate::commons::ids::CallbackId;
use crate::commons::input::{Keybind, Keybinds, Modifiers};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum View {
    Tree,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeybindMode {
    Global,
    View(View),
}

/// A bound key resolves to either a compiled action or, for bindings the
/// script registered, a handle the [`crate::script::ScriptHost`] can run.
#[derive(Clone)]
pub enum ActionTarget {
    Static(Action),
    Dynamic(CallbackId),
}

#[derive(Clone, PartialEq, Eq)]
pub enum KeySource {
    Keybinds(Keybinds),
    Keybind(Keybind),
}

impl KeySource {
    pub fn matches(&self, key: &KeyEvent) -> bool {
        match self {
            KeySource::Keybinds(kb) => kb.matches(key),
            KeySource::Keybind(kb) => kb.matches(key),
        }
    }

    /// Whether any of this source's keys matches the canonical identity —
    /// the same equivalence `remove` and `override_binding` use (finding
    /// 6.11: register and remove must agree on what "the same key" is).
    fn has_canonical(&self, canonical: &[(Modifiers, String)]) -> bool {
        canonical_keys(self).iter().any(|c| canonical.contains(c))
    }
}

/// The canonical identities a source carries, one per key.
fn canonical_keys(source: &KeySource) -> Vec<(Modifiers, String)> {
    match source {
        KeySource::Keybinds(ks) => ks.0.iter().map(|k| k.canonical()).collect(),
        KeySource::Keybind(kb) => vec![kb.canonical()],
    }
}

impl KeySource {
    /// True when this source binds the same canonical key as `kb`.
    pub fn binds(&self, kb: &Keybind) -> bool {
        let target = kb.canonical();
        match self {
            KeySource::Keybind(k) => k.canonical() == target,
            KeySource::Keybinds(ks) => ks.0.iter().any(|k| k.canonical() == target),
        }
    }

    /// Renders every chord in this source for display.
    pub fn display(&self) -> Vec<String> {
        match self {
            KeySource::Keybind(k) => vec![k.display()],
            KeySource::Keybinds(ks) => ks.0.iter().map(Keybind::display).collect(),
        }
    }
}

impl From<Keybinds> for KeySource {
    fn from(kb: Keybinds) -> Self {
        KeySource::Keybinds(kb)
    }
}

impl From<Keybind> for KeySource {
    fn from(kb: Keybind) -> Self {
        KeySource::Keybind(kb)
    }
}

/// One displayable binding: its mode, its chords, and its target labels.
pub struct KeybindEntry {
    pub mode: KeybindMode,
    pub keys: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Clone)]
pub struct KeybindRegistry {
    /// Bindings in registration order; dispatch visits **every** match in
    /// this order — `Global` and the active view's bindings both fire, and a
    /// key bound twice runs twice (a script binding on an already-bound key
    /// extends it instead of shadowing it — see also finding 6.11). The
    /// registry-level test pins the order.
    pub bindings: Vec<(KeybindMode, KeySource, Vec<ActionTarget>)>,
    /// Optional human labels for script callbacks, keyed by callback id.
    labels: std::collections::HashMap<u64, String>,
}

impl Default for KeybindRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl KeybindRegistry {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            labels: std::collections::HashMap::new(),
        }
    }

    /// Attaches a display label to a script callback id.
    pub fn label_callback(&mut self, id: CallbackId, label: String) {
        self.labels.insert(id.0, label);
    }

    /// Every binding, flattened for display.
    pub fn entries(&self) -> Vec<KeybindEntry> {
        self.bindings
            .iter()
            .map(|(mode, source, targets)| KeybindEntry {
                mode: mode.clone(),
                keys: source.display(),
                labels: targets.iter().map(|t| self.target_label(t)).collect(),
            })
            .collect()
    }

    /// A single target's display label.
    fn target_label(&self, target: &ActionTarget) -> String {
        match target {
            ActionTarget::Static(action) => {
                // Show the leaf name only; the section already groups by view.
                let full = action.describe();
                full.rsplit('.').next().unwrap_or(&full).to_string()
            }
            ActionTarget::Dynamic(id) => self
                .labels
                .get(&id.0)
                .cloned()
                .unwrap_or_else(|| "<script>".to_string()),
        }
    }

    pub fn add_binding(&mut self, mode: KeybindMode, kb: KeySource, target: ActionTarget) {
        let canonical = canonical_keys(&kb);
        // Identity is the canonical key (finding 6.11): `Char(" ")` and
        // `Code("space")` are one binding, the same identity `remove` uses.
        if let Some(entry) = self
            .bindings
            .iter_mut()
            .find(|(m, source, _)| m == &mode && source.has_canonical(&canonical))
        {
            entry.2.push(target);
        } else {
            self.bindings.push((mode, kb, vec![target]));
        }
    }

    /// Rebinds `kb` in `mode`, replacing whatever the same canonical key was
    /// bound to before — defaults included.
    ///
    /// A script `keybind` is a rebind (finding 6.11): appending it left the
    /// built-in target live too, so `q` both quit *and* ran the script.
    /// Identity is the canonical key, so a script `space` replaces a `' '`
    /// default and vice versa.
    pub fn override_binding(&mut self, mode: KeybindMode, kb: KeySource, target: ActionTarget) {
        let canonical = canonical_keys(&kb);
        // A `Global` rebind owns the key everywhere: view-mode bindings on
        // the same key would otherwise keep firing alongside the script's.
        // A view-scoped rebind clears its own view (a `Global` default on
        // the same key still co-fires there — rebind both explicitly when
        // that matters).
        let key_wide = matches!(mode, KeybindMode::Global);
        self.bindings.retain_mut(|(m, source, _)| {
            if !key_wide && *m != mode {
                return true;
            }
            match source {
                KeySource::Keybind(k) => !canonical.contains(&k.canonical()),
                KeySource::Keybinds(ks) => {
                    ks.0.retain(|k| !canonical.contains(&k.canonical()));
                    !ks.0.is_empty()
                }
            }
        });
        self.add_binding(mode, kb, target);
    }

    /// Removes every binding, including the built-in defaults.
    pub fn clear(&mut self) {
        self.bindings.clear();
        self.labels.clear();
    }

    /// Restores the built-in defaults, discarding every custom binding.
    pub fn reset(&mut self) {
        *self = default_keybinds();
    }

    /// Removes every binding that targets `kb`, across all modes.
    pub fn remove(&mut self, kb: &Keybind) {
        let target = kb.canonical();
        self.bindings.retain_mut(|(_, source, _)| match source {
            KeySource::Keybind(k) => k.canonical() != target,
            KeySource::Keybinds(ks) => {
                ks.0.retain(|k| k.canonical() != target);
                !ks.0.is_empty()
            }
        });
    }

    pub fn register<K, T>(mut self, kb: K, act: T) -> Self
    where
        K: Into<KeySource>,
        T: Into<Action>,
    {
        self.add_binding(
            KeybindMode::Global,
            kb.into(),
            ActionTarget::Static(act.into()),
        );
        self
    }

    /// Binds a script callback in `Global` mode.
    ///
    /// This is a rebind (finding 6.11): the key's previous binding — a
    /// built-in default included — is replaced, not extended, so a script
    /// `q` stops `quit` from also firing.
    pub fn register_fn<K>(mut self, kb: K, id: CallbackId) -> Self
    where
        K: Into<KeySource>,
    {
        self.override_binding(KeybindMode::Global, kb.into(), ActionTarget::Dynamic(id));
        self
    }

    /// Binds a script callback in `mode`; a rebind like [`Self::register_fn`].
    pub fn register_fn_mode<K>(mut self, mode: KeybindMode, kb: K, id: CallbackId) -> Self
    where
        K: Into<KeySource>,
    {
        self.override_binding(mode, kb.into(), ActionTarget::Dynamic(id));
        self
    }

    pub fn on_mode<F>(mut self, mode: KeybindMode, f: F) -> Self
    where
        F: FnOnce(ModeRegistryBuilder) -> ModeRegistryBuilder,
    {
        let builder = f(ModeRegistryBuilder {
            mode,
            bindings: Vec::new(),
        });
        for (m, kb, mut targets) in builder.bindings {
            // Same canonical-key identity as `add_binding` (finding 6.11):
            // `space`-spelled and `' '`-spelled registrations of one mode
            // are one binding.
            if let Some(entry) = self
                .bindings
                .iter_mut()
                .find(|(em, ek, _)| em == &m && ek.has_canonical(&canonical_keys(&kb)))
            {
                entry.2.append(&mut targets);
            } else {
                self.bindings.push((m, kb, targets));
            }
        }
        self
    }
}

pub struct ModeRegistryBuilder {
    mode: KeybindMode,
    bindings: Vec<(KeybindMode, KeySource, Vec<ActionTarget>)>,
}

impl ModeRegistryBuilder {
    fn add_binding(&mut self, kb: KeySource, target: ActionTarget) {
        let canonical = canonical_keys(&kb);
        // Same canonical identity as the registry-level merge (6.11).
        if let Some(entry) = self
            .bindings
            .iter_mut()
            .find(|(m, source, _)| m == &self.mode && source.has_canonical(&canonical))
        {
            entry.2.push(target);
        } else {
            self.bindings.push((self.mode.clone(), kb, vec![target]));
        }
    }

    pub fn register<K, T>(mut self, kb: K, act: T) -> Self
    where
        K: Into<KeySource>,
        T: Into<Action>,
    {
        self.add_binding(kb.into(), ActionTarget::Static(act.into()));
        self
    }

    pub fn register_fn<K>(mut self, kb: K, id: CallbackId) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(kb.into(), ActionTarget::Dynamic(id));
        self
    }
}

pub fn default_keybinds() -> KeybindRegistry {
    KeybindRegistry::new()
        .register(Keybinds::code(KeyCode::Enter), GlobalActions::confirm)
        .register(Keybinds::char('q').with_ctrl('c'), GlobalActions::quit)
        .register(Keybinds::char(':'), GlobalActions::open_command_mode)
        .register(Keybinds::char('?'), GlobalActions::help)
        .register(Keybinds::char('u'), GlobalActions::undo)
        .register(Keybinds::new().with_ctrl('r'), GlobalActions::redo)
        .register(
            Keybinds::char('h').with_code(KeyCode::Esc),
            ViewFileActions::close,
        )
        .on_mode(KeybindMode::View(View::File), |r| {
            r.register(
                Keybinds::code(KeyCode::Left).with_ctrl('h'),
                ViewFileScrollActions::left(1),
            )
            .register(
                Keybinds::code(KeyCode::Right).with_ctrl('l'),
                ViewFileScrollActions::right(1),
            )
            .register(
                Keybinds::char('j').with_code(KeyCode::Down),
                ViewFileCursorActions::down(1),
            )
            .register(
                Keybinds::char('k').with_code(KeyCode::Up),
                ViewFileCursorActions::up(1),
            )
            .register(
                Keybinds::new().with_ctrl('d'),
                ViewFileCursorActions::half_page_down,
            )
            .register(
                Keybinds::new().with_ctrl('u'),
                ViewFileCursorActions::half_page_up,
            )
            .register(
                Keybinds::new().with_ctrl('b'),
                ViewFileCursorActions::page_up,
            )
            .register(
                Keybinds::new().with_ctrl('f'),
                ViewFileCursorActions::page_down,
            )
            .register(
                Keybinds::code(KeyCode::PageUp),
                ViewFileCursorActions::page_up,
            )
            .register(
                Keybinds::code(KeyCode::PageDown),
                ViewFileCursorActions::page_down,
            )
            .register(Keybinds::char('G'), ViewFileCursorActions::bottom)
            .register(Keybinds::char('g'), ViewFileCursorActions::top)
            .register(Keybinds::char('n'), ViewFileNavActions::next_hunk)
            .register(Keybinds::char('N'), ViewFileNavActions::prev_hunk)
            .register(Keybinds::char(' '), ViewFileStagingActions::toggle)
            .register(Keybinds::char('t'), ViewFileStagingActions::toggle_line)
            .register(Keybinds::char('s'), ViewFileStagingActions::split)
            .register(Keybinds::char('i'), ViewFileStagingActions::invert)
            .register(Keybinds::char('z'), ViewFileFoldActions::toggle)
        })
        .on_mode(KeybindMode::View(View::Tree), |r| {
            r.register(
                Keybinds::new().with_ctrl('d'),
                ViewTreeCursorActions::down(20),
            )
            .register(
                Keybinds::new().with_ctrl('u'),
                ViewTreeCursorActions::up(20),
            )
            .register(
                Keybinds::new().with_ctrl('b'),
                ViewTreeCursorActions::page_up,
            )
            .register(
                Keybinds::new().with_ctrl('f'),
                ViewTreeCursorActions::page_down,
            )
            .register(
                Keybinds::code(KeyCode::PageUp),
                ViewTreeCursorActions::page_up,
            )
            .register(
                Keybinds::code(KeyCode::PageDown),
                ViewTreeCursorActions::page_down,
            )
            .register(
                Keybinds::char('j').with_code(KeyCode::Down),
                ViewTreeCursorActions::down(1),
            )
            .register(
                Keybinds::char('k').with_code(KeyCode::Up),
                ViewTreeCursorActions::up(1),
            )
            .register(Keybinds::char('G'), ViewTreeCursorActions::bottom)
            .register(Keybinds::char('g'), ViewTreeCursorActions::top)
            .register(
                Keybinds::char('l').with_code(KeyCode::Right),
                ViewTreeActions::open_selected,
            )
            .register(
                Keybinds::char('h').with_code(KeyCode::Left),
                ViewTreeDirectoryActions::collapse,
            )
            .register(Keybinds::char(' '), ViewTreeStagingActions::toggle_selected)
            .register(Keybinds::char('i'), ViewTreeStagingActions::invert)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{
        Action, Actions, GlobalActions, ViewActions, ViewFileActions, ViewFileCursorActions,
    };
    use crate::commons::input::Keybind;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    /// Dispatch visits every matching binding — global and active-view alike
    /// — in registration order. This is the documented keystroke semantics
    /// (finding 6.12); if a later rewrite changes it, this test is the
    /// place it gets consciously renegotiated.
    #[test]
    fn dispatch_visits_matches_in_registration_order() {
        let mut reg = KeybindRegistry::default();
        let q = || KeySource::Keybind(Keybind::parse("q"));
        let target = |n: u64| ActionTarget::Dynamic(CallbackId(n));

        // A global binding first, then a file-view binding on the same key;
        // a tree-view binding on that key must not fire in the file view.
        reg.bindings
            .push((KeybindMode::Global, q(), vec![target(1)]));
        reg.bindings
            .push((KeybindMode::View(View::File), q(), vec![target(2)]));
        reg.bindings
            .push((KeybindMode::View(View::Tree), q(), vec![target(3)]));

        let active = KeybindMode::View(View::File);
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let fired: Vec<u64> = reg
            .bindings
            .iter()
            .filter(|(mode, kb, _)| {
                (matches!(mode, KeybindMode::Global) || *mode == active) && kb.matches(&key)
            })
            .flat_map(|(_, _, targets)| targets.iter())
            .filter_map(|t| match t {
                ActionTarget::Dynamic(id) => Some(id.0),
                _ => None,
            })
            .collect();

        assert_eq!(
            fired,
            vec![1, 2],
            "global and view matches fire in registration order"
        );
    }

    /// A script rebind replaces the built-in binding on the same key —
    /// dispatch must not fire both (finding 6.11) — and key identity is the
    /// canonical form, so a script `space` rebinding replaces a `' '`
    /// default.
    #[test]
    fn script_rebind_replaces_the_default_binding() {
        let registry = default_keybinds();
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let targets_of = |reg: &KeybindRegistry, space: &KeyEvent| -> Vec<ActionTarget> {
            reg.bindings
                .iter()
                .filter(|(_, source, _)| source.matches(space))
                .flat_map(|(_, _, targets)| targets.iter().cloned())
                .collect()
        };

        let before = targets_of(&registry, &space);
        assert!(!before.is_empty(), "the defaults bind space somehow");

        let registry = registry.register_fn(Keybind::parse("space"), CallbackId(7));
        let after = targets_of(&registry, &space);
        assert_eq!(after.len(), 1, "the rebind replaces the default target");
        assert!(
            matches!(after[0], ActionTarget::Dynamic(id) if id.0 == 7),
            "the surviving target is the script's"
        );

        // register and remove agree on canonical identity: removing `' '`
        // also removes `space`-spelled bindings in the defaults.
        let mut fresh = default_keybinds();
        fresh.remove(&Keybind::Char(" ".to_string()));
        assert!(
            targets_of(&fresh, &space).is_empty(),
            "removing one spelling removes the whole key"
        );
    }

    /// Registering the same key under different spellings merges into one
    /// entry — the canonical identity `remove` already used (finding 6.11).
    #[test]
    fn register_and_remove_agree_on_identity() {
        let mut registry = KeybindRegistry::default();
        registry.add_binding(
            KeybindMode::Global,
            KeySource::Keybinds(Keybinds(vec![Keybind::Char(" ".to_string())])),
            ActionTarget::Dynamic(CallbackId(1)),
        );
        registry.add_binding(
            KeybindMode::Global,
            KeySource::Keybind(Keybind::parse("space")),
            ActionTarget::Dynamic(CallbackId(2)),
        );

        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let fired: Vec<u64> = registry
            .bindings
            .iter()
            .filter(|(_, source, _)| source.matches(&space))
            .flat_map(|(_, _, targets)| targets.iter())
            .filter_map(|t| match t {
                ActionTarget::Dynamic(id) => Some(id.0),
                _ => None,
            })
            .collect();
        assert_eq!(fired, vec![1, 2], "both targets fire; they share one entry");

        // Removing by either spelling drops the whole thing.
        registry.remove(&Keybind::parse("space"));
        assert!(
            !registry
                .bindings
                .iter()
                .any(|(_, source, _)| source.matches(&space)),
            "the dead remnant must be gone, not half-removed"
        );
    }

    #[test]
    fn actions_describe_themselves() {
        let quit = Action(Actions::global(GlobalActions::quit));
        assert_eq!(quit.describe(), "global.quit");

        let down = Action(Actions::view(ViewActions::file(ViewFileActions::cursor(
            ViewFileCursorActions::down(5),
        ))));
        assert_eq!(down.describe(), "view.file.cursor.down(5)");
    }

    #[test]
    fn entries_carry_action_labels() {
        let reg = default_keybinds();
        let entries = reg.entries();
        assert!(
            entries.iter().any(|e| e.keys.contains(&"enter".to_string())
                && e.labels.iter().any(|l| l == "confirm")),
            "enter must list the confirm label"
        );
    }

    #[test]
    fn remove_keeps_sibling_keys_in_a_shared_source() {
        let mut reg = default_keybinds();
        reg.remove(&Keybind::parse("esc"));

        let h = Keybind::parse("h");
        let esc = Keybind::parse("esc");
        assert!(
            reg.bindings.iter().any(|(_, k, _)| k.binds(&h)),
            "'h' must survive removing 'esc' from the shared source"
        );
        assert!(
            !reg.bindings.iter().any(|(_, k, _)| k.binds(&esc)),
            "'esc' must be removed"
        );
    }
}
