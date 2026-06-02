use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keybind {
    Char(String),
    Code(String),
    Combination(Modifier, String),
    CodeCombination(Modifier, String),
}

impl Keybind {
    pub fn parse(s: &str) -> Self {
        let parts: Vec<&str> = s.split(['-', '+']).map(|p| p.trim()).collect();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = String::new();

        for part in parts {
            let lower = part.to_lowercase();
            match lower.as_str() {
                "ctrl" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                k => key = k.to_string(),
            }
        }

        let is_code = Self::string_to_code(&key).is_some();
        let modifier = if ctrl {
            Some(Modifier::Ctrl)
        } else if alt {
            Some(Modifier::Alt)
        } else if shift {
            Some(Modifier::Shift)
        } else {
            None
        };

        if let Some(m) = modifier {
            if is_code {
                Keybind::CodeCombination(m, key)
            } else {
                Keybind::Combination(m, key)
            }
        } else {
            if is_code {
                Keybind::Code(key)
            } else {
                Keybind::Char(key)
            }
        }
    }

    fn string_to_code(s: &str) -> Option<KeyCode> {
        match s.to_lowercase().as_str() {
            "enter" | "return" => Some(KeyCode::Enter),
            "esc" | "escape" => Some(KeyCode::Esc),
            "backspace" => Some(KeyCode::Backspace),
            "up" => Some(KeyCode::Up),
            "down" => Some(KeyCode::Down),
            "left" => Some(KeyCode::Left),
            "right" => Some(KeyCode::Right),
            "tab" => Some(KeyCode::Tab),
            "space" => Some(KeyCode::Char(' ')),
            _ => None,
        }
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        let mods = event.modifiers;
        match self {
            Keybind::Char(s) => {
                if let Some(c) = s.chars().next() {
                    event.code == KeyCode::Char(c) && mods.is_empty()
                } else {
                    false
                }
            }
            Keybind::Code(s) => {
                if let Some(code) = Self::string_to_code(s) {
                    event.code == code && mods.is_empty()
                } else {
                    false
                }
            }
            Keybind::Combination(Modifier::Ctrl, s) => {
                if let Some(c) = s.chars().next() {
                    event.code == KeyCode::Char(c) && mods.contains(KeyModifiers::CONTROL)
                } else {
                    false
                }
            }
            Keybind::Combination(Modifier::Alt, s) => {
                if let Some(c) = s.chars().next() {
                    event.code == KeyCode::Char(c) && mods.contains(KeyModifiers::ALT)
                } else {
                    false
                }
            }
            Keybind::Combination(Modifier::Shift, s) => {
                if let Some(c) = s.chars().next() {
                    event.code == KeyCode::Char(c) && mods.contains(KeyModifiers::SHIFT)
                } else {
                    false
                }
            }
            Keybind::CodeCombination(Modifier::Ctrl, s) => {
                if let Some(code) = Self::string_to_code(s) {
                    event.code == code && mods.contains(KeyModifiers::CONTROL)
                } else {
                    false
                }
            }
            Keybind::CodeCombination(Modifier::Alt, s) => {
                if let Some(code) = Self::string_to_code(s) {
                    event.code == code && mods.contains(KeyModifiers::ALT)
                } else {
                    false
                }
            }
            Keybind::CodeCombination(Modifier::Shift, s) => {
                if let Some(code) = Self::string_to_code(s) {
                    event.code == code && mods.contains(KeyModifiers::SHIFT)
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Keybinds(pub Vec<Keybind>);

impl Keybinds {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn char(c: char) -> Self {
        Self(vec![Keybind::Char(c.to_string())])
    }
    pub fn code(c: KeyCode) -> Self {
        Self(vec![Keybind::Code(Self::code_to_string(c))])
    }
    pub fn with_char(mut self, c: char) -> Self {
        self.0.push(Keybind::Char(c.to_string()));
        self
    }

    pub fn with_shift(mut self, c: char) -> Self {
        self.0
            .push(Keybind::Combination(Modifier::Shift, c.to_string()));
        self
    }

    pub fn with_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::Code(Self::code_to_string(c)));
        self
    }
    pub fn with_ctrl(mut self, c: char) -> Self {
        self.0
            .push(Keybind::Combination(Modifier::Ctrl, c.to_string()));
        self
    }
    pub fn with_ctrl_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::CodeCombination(
            Modifier::Ctrl,
            Self::code_to_string(c),
        ));
        self
    }

    fn code_to_string(c: KeyCode) -> String {
        match c {
            KeyCode::Enter => "enter",
            KeyCode::Esc => "esc",
            KeyCode::Backspace => "backspace",
            KeyCode::Up => "up",
            KeyCode::Down => "down",
            KeyCode::Left => "left",
            KeyCode::Right => "right",
            KeyCode::Tab => "tab",
            KeyCode::Char(' ') => "space",
            _ => "",
        }
        .to_string()
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        self.0.iter().any(|kb| kb.matches(event))
    }
}

// --- Generic Registry Mechanics ---
pub struct ActionSetter<'a, A> {
    action: &'a mut Option<A>,
}

impl<'a, A> ActionSetter<'a, A> {
    pub fn set_action(&mut self, action: A) {
        *self.action = Some(action);
    }
}

pub struct KeybindMatcher<'a, A> {
    key: KeyEvent,
    action: &'a mut Option<A>,
    handled: &'a mut bool,
}

impl<'a, A> KeybindMatcher<'a, A> {
    pub fn matches<F>(&mut self, binds: &Keybinds, mut cb: F)
    where
        F: FnMut(),
    {
        if *self.handled {
            return;
        }
        if binds.matches(&self.key) {
            *self.handled = true;
            cb();
        }
    }

    pub fn matches_action<F>(&mut self, binds: &Keybinds, mut cb: F)
    where
        F: FnMut(&mut ActionSetter<A>),
    {
        if *self.handled {
            return;
        }
        if binds.matches(&self.key) {
            *self.handled = true;
            let mut setter = ActionSetter {
                action: self.action,
            };
            cb(&mut setter);
        }
    }
}

pub trait KeybindHandler<C, A> {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<A>);
}

pub struct KeybindRegistry<'reg, C, A> {
    ctx: &'reg mut C,
    key: KeyEvent,
    action: Option<A>,
    handled: bool,
}

impl<'reg, C, A> KeybindRegistry<'reg, C, A> {
    pub fn new(ctx: &'reg mut C, key: KeyEvent) -> Self {
        Self {
            ctx,
            key,
            action: None,
            handled: false,
        }
    }

    pub fn process<H>(mut self, handler: &H) -> Self
    where
        H: KeybindHandler<C, A>,
    {
        if !self.handled {
            let mut matcher = KeybindMatcher {
                key: self.key,
                action: &mut self.action,
                handled: &mut self.handled,
            };
            handler.handle(self.ctx, &mut matcher);
        }
        self
    }

    pub fn execute(self) -> (Option<A>, bool) {
        (self.action, self.handled)
    }
}
