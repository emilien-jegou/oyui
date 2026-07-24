use crossterm::event::{KeyCode, KeyEvent};
use rune::runtime::Function;
use std::rc::Rc;

use crate::actions::{
    Action, GlobalActions, ViewFileActions, ViewFileCursorActions, ViewFileFoldActions,
    ViewFileNavActions, ViewFileScrollActions, ViewFileStagingActions, ViewTreeActions,
    ViewTreeCursorActions, ViewTreeDirectoryActions, ViewTreeStagingActions,
};
use crate::commons::input::{Keybind, Keybinds};

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

#[derive(Clone)]
pub enum ActionTarget {
    Static(Action),
    Dynamic(Rc<Function>),
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

    pub fn overlaps_with(&self, other: &KeySource) -> bool {
        match (self, other) {
            (KeySource::Keybind(a), KeySource::Keybind(b)) => a == b,
            (KeySource::Keybinds(keybinds), KeySource::Keybind(keybind)) => {
                keybinds.0.iter().any(|b| b == keybind)
            }
            (a @ KeySource::Keybinds(_), b @ KeySource::Keybinds(_)) => b.overlaps_with(a),
            (a @ KeySource::Keybind(_), b @ KeySource::Keybinds(_)) => b.overlaps_with(a),
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

impl IntoIterator for KeySource {
    type Item = Keybind;
    type IntoIter = std::vec::IntoIter<Keybind>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            KeySource::Keybinds(Keybinds(binds)) => binds.into_iter(),
            KeySource::Keybind(bind) => vec![bind].into_iter(),
        }
    }
}

impl FromIterator<Keybind> for Option<KeySource> {
    fn from_iter<I: IntoIterator<Item = Keybind>>(iter: I) -> Self {
        let binds: Vec<Keybind> = iter.into_iter().collect();
        if binds.is_empty() {
            None
        } else {
            Some(KeySource::Keybinds(Keybinds(binds)))
        }
    }
}

#[derive(Clone)]
pub struct KeybindRegistry {
    pub bindings: Vec<(KeybindMode, KeySource, Vec<ActionTarget>)>,
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

    pub fn register_fn<K>(mut self, kb: K, func: Rc<Function>) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(KeybindMode::Global, kb.into(), ActionTarget::Dynamic(func));
        self
    }

    pub fn register_fn_mode<K>(mut self, mode: KeybindMode, kb: K, func: Rc<Function>) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(mode, kb.into(), ActionTarget::Dynamic(func));
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

    /// Supply default key bindings from another KeybindRegistry. Skips defaults for keys or key
    /// combinations that already have a binding.
    pub fn with_defaults(&mut self, defaults: KeybindRegistry) {
        for (mode, source, targets) in defaults.bindings {
            // In case the default has multiple possible key bindings, filter down to bindings that
            // aren't already taken.
            let unclaimed_keybinds: Option<KeySource> = source
                .into_iter()
                .filter(|binding| !self.already_bound(&mode, binding))
                .collect();
            if let Some(kb) = unclaimed_keybinds {
                for target in targets {
                    self.add_binding(mode.clone(), kb.clone(), target)
                }
            }
        }
    }

    fn already_bound(&self, mode: &KeybindMode, binding: &Keybind) -> bool {
        let source = KeySource::Keybind(binding.clone());
        self.bindings.iter().any(|(m, s, _)| {
            (m == mode || *mode == KeybindMode::Global || *m == KeybindMode::Global)
                && s.overlaps_with(&source)
        })
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

    pub fn register_fn<K>(mut self, kb: K, func: Rc<Function>) -> Self
    where
        K: Into<KeySource>,
    {
        self.add_binding(kb.into(), ActionTarget::Dynamic(func));
        self
    }
}

pub fn default_keybinds() -> KeybindRegistry {
    KeybindRegistry::new()
        .register(Keybinds::code(KeyCode::Enter), GlobalActions::confirm)
        .register(Keybinds::char('q').with_ctrl('c'), GlobalActions::quit)
        .register(Keybinds::char(':'), GlobalActions::open_command_mode)
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
