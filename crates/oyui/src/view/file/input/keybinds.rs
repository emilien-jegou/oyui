use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keybind {
    Char(char),
    Code(KeyCode),
    Combination(Modifier, char),
    CodeCombination(Modifier, KeyCode),
}

impl Keybind {
    pub fn matches(&self, event: &KeyEvent) -> bool {
        let mods = event.modifiers;
        match self {
            Keybind::Char(c) => event.code == KeyCode::Char(*c) && mods.is_empty(),
            Keybind::Code(code) => event.code == *code && mods.is_empty(),
            Keybind::Combination(Modifier::Ctrl, c) => {
                event.code == KeyCode::Char(*c) && mods.contains(KeyModifiers::CONTROL)
            }
            Keybind::Combination(Modifier::Alt, c) => {
                event.code == KeyCode::Char(*c) && mods.contains(KeyModifiers::ALT)
            }
            Keybind::Combination(Modifier::Shift, c) => {
                event.code == KeyCode::Char(*c) && mods.contains(KeyModifiers::SHIFT)
            }
            Keybind::CodeCombination(Modifier::Ctrl, code) => {
                event.code == *code && mods.contains(KeyModifiers::CONTROL)
            }
            Keybind::CodeCombination(Modifier::Alt, code) => {
                event.code == *code && mods.contains(KeyModifiers::ALT)
            }
            Keybind::CodeCombination(Modifier::Shift, code) => {
                event.code == *code && mods.contains(KeyModifiers::SHIFT)
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Keybinds(pub Vec<Keybind>);

impl Keybinds {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn char(c: char) -> Self {
        Self(vec![Keybind::Char(c)])
    }

    pub fn code(c: KeyCode) -> Self {
        Self(vec![Keybind::Code(c)])
    }

    pub fn with_char(mut self, c: char) -> Self {
        self.0.push(Keybind::Char(c));
        self
    }

    pub fn with_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::Code(c));
        self
    }

    pub fn with_ctrl(mut self, c: char) -> Self {
        self.0.push(Keybind::Combination(Modifier::Ctrl, c));
        self
    }

    pub fn with_ctrl_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::CodeCombination(Modifier::Ctrl, c));
        self
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.0.iter().any(|kb| kb.matches(event))
    }
}
