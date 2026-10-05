use crossterm::event::{KeyCode, KeyEvent};

use crate::actions::{
    Action, GlobalActions, ViewFileActions, ViewFileCursorActions, ViewFileFoldActions,
    ViewFileNavActions, ViewFileScrollActions, ViewFileStagingActions, ViewTreeActions,
    ViewTreeCursorActions, ViewTreeDirectoryActions, ViewTreeStagingActions,
};
use crate::commons::input::{Keybind, Keybinds};
use crate::script::CallbackId;

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

    fn add_binding(&mut self, mode: KeybindMode, kb: KeySource, target: ActionTarget) {
        if let Some(entry) = self
            .bindings
            .iter_mut()
            .find(|(m, k, _)| m == &mode && k == &kb)
        {
            entry.2.push(target);
        } else {
            self.bindings.push((mode, kb, vec![target]));
        }
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

    pub fn register_fn<K>(mut self, kb: K, id: CallbackId) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(KeybindMode::Global, kb.into(), ActionTarget::Dynamic(id));
        self
    }

    pub fn register_fn_mode<K>(mut self, mode: KeybindMode, kb: K, id: CallbackId) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(mode, kb.into(), ActionTarget::Dynamic(id));
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
            if let Some(entry) = self
                .bindings
                .iter_mut()
                .find(|(em, ek, _)| em == &m && ek == &kb)
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
        if let Some(entry) = self
            .bindings
            .iter_mut()
            .find(|(m, k, _)| m == &self.mode && k == &kb)
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
