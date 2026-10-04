use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A set of held modifiers; any combination is representable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keybind {
    Char(String),
    Code(String),
    Combination(Modifiers, String),
    CodeCombination(Modifiers, String),
}

impl Keybind {
    pub fn parse(s: &str) -> Self {
        let parts: Vec<&str> = s.split(['-', '+']).map(|p| p.trim()).collect();
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = String::new();

        for part in parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => key = part.to_string(), // Preserve the original case of the character/code key
            }
        }

        let mods = Modifiers { ctrl, alt, shift };
        let is_code = Self::string_to_code(&key).is_some();

        if !mods.is_empty() {
            if is_code {
                Keybind::CodeCombination(mods, key)
            } else {
                Keybind::Combination(mods, key)
            }
        } else if is_code {
            Keybind::Code(key)
        } else {
            Keybind::Char(key)
        }
    }

    /// Parses `s`, rejecting unknown key names and malformed chords.
    pub fn parse_checked(s: &str) -> Result<Self, String> {
        let kb = Self::parse(s);
        if kb.is_valid() {
            Ok(kb)
        } else {
            Err(format!("invalid keybind '{s}'"))
        }
    }

    /// True when the parsed key names a real key.
    fn is_valid(&self) -> bool {
        match self {
            Keybind::Char(s) => s.chars().count() == 1,
            Keybind::Code(s) => Self::string_to_code(s).is_some(),
            Keybind::Combination(_, s) => s.chars().count() == 1,
            Keybind::CodeCombination(_, s) => Self::string_to_code(s).is_some(),
        }
    }

    /// Canonical identity used to match equivalent spellings (`space` == `' '`).
    pub fn canonical(&self) -> (Modifiers, String) {
        match self {
            Keybind::Char(s) => (Modifiers::default(), normalize_key(s)),
            Keybind::Code(s) => (Modifiers::default(), normalize_code(s)),
            Keybind::Combination(m, s) => (*m, normalize_key(s)),
            Keybind::CodeCombination(m, s) => (*m, normalize_code(s)),
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
            "pageup" => Some(KeyCode::PageUp),
            "pagedown" => Some(KeyCode::PageDown),
            "tab" => Some(KeyCode::Tab),
            "space" => Some(KeyCode::Char(' ')),
            _ => None,
        }
    }

    pub fn matches(&self, event: &KeyEvent) -> bool {
        match self {
            Keybind::Char(s) => {
                let Some(c) = s.chars().next() else {
                    return false;
                };
                if event.code != KeyCode::Char(c) {
                    return false;
                }
                event.modifiers.is_empty() || event.modifiers == KeyModifiers::SHIFT
            }
            Keybind::Code(s) => Self::string_to_code(s)
                .is_some_and(|code| event.code == code && event.modifiers.is_empty()),
            Keybind::Combination(mods, s) => {
                let Some(c) = s.chars().next() else {
                    return false;
                };
                modifiers_match(*mods, event.modifiers) && matches_char(event.code, c, mods.shift)
            }
            Keybind::CodeCombination(mods, s) => {
                let Some(code) = Self::string_to_code(s) else {
                    return false;
                };
                modifiers_match(*mods, event.modifiers) && event.code == code
            }
        }
    }
}

/// True when the held modifiers exactly equal the bound ones.
fn modifiers_match(bound: Modifiers, held: KeyModifiers) -> bool {
    bound.ctrl == held.contains(KeyModifiers::CONTROL)
        && bound.alt == held.contains(KeyModifiers::ALT)
        && bound.shift == held.contains(KeyModifiers::SHIFT)
}

/// Compares a key event's code to a bound char, tolerating shift casing.
fn matches_char(code: KeyCode, bound: char, shift: bool) -> bool {
    match code {
        KeyCode::Char(c) => c == bound || (shift && c.eq_ignore_ascii_case(&bound)),
        _ => false,
    }
}

/// Normalizes a char token so `' '` and `space` compare equal.
fn normalize_key(s: &str) -> String {
    match s {
        " " => "space".to_string(),
        other => other.to_string(),
    }
}

/// Normalizes a named key token.
fn normalize_code(s: &str) -> String {
    s.to_lowercase()
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
        self.0.push(Keybind::Combination(
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            c.to_string(),
        ));
        self
    }

    pub fn with_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::Code(Self::code_to_string(c)));
        self
    }

    pub fn with_ctrl(mut self, c: char) -> Self {
        self.0.push(Keybind::Combination(
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
            c.to_string(),
        ));
        self
    }

    pub fn with_ctrl_code(mut self, c: KeyCode) -> Self {
        self.0.push(Keybind::CodeCombination(
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
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
            KeyCode::PageUp => "pageup",
            KeyCode::PageDown => "pagedown",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn multi_modifier_chords_parse_and_match() {
        let bind = Keybind::parse("ctrl-shift-j");
        assert!(bind.matches(&key(
            KeyCode::Char('J'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )));
        assert!(!bind.matches(&key(KeyCode::Char('j'), KeyModifiers::CONTROL)));
        assert!(!bind.matches(&key(
            KeyCode::Char('j'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        )));
    }

    #[test]
    fn space_and_named_space_are_canonical() {
        let char_space = Keybind::Char(" ".to_string());
        assert_eq!(Keybind::parse("space").canonical(), char_space.canonical());
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(Keybind::parse_checked("bogus").is_err());
        assert!(Keybind::parse_checked("ctrl-shift-j").is_ok());
    }
}
