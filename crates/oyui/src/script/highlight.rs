//! Engine-side mirror of [`LineHighlightMode`].
//!
//! The core type carries no engine traits; this mirror is what the engine
//! sees, and the [`ScriptRepr`] bridge converts between the two.

use crate::config::LineHighlightMode;
use oyui_rune_actions::ScriptRepr;
use rune::Any;

/// Engine-facing shape of [`LineHighlightMode`], exposed to scripts under the
/// same `LineHighlightMode` name so existing configs keep working.
#[derive(Debug, Clone, Copy, Any)]
#[rune(name = LineHighlightMode)]
pub enum ScriptLineHighlightMode {
    #[rune(constructor)]
    None,
    #[rune(constructor)]
    Solid,
    #[rune(constructor)]
    Gradient(#[rune(get)] f64),
}

impl From<LineHighlightMode> for ScriptLineHighlightMode {
    fn from(mode: LineHighlightMode) -> Self {
        match mode {
            LineHighlightMode::None => Self::None,
            LineHighlightMode::Solid => Self::Solid,
            LineHighlightMode::Gradient(opacity) => Self::Gradient(opacity),
        }
    }
}

impl From<ScriptLineHighlightMode> for LineHighlightMode {
    fn from(mode: ScriptLineHighlightMode) -> Self {
        match mode {
            ScriptLineHighlightMode::None => Self::None,
            ScriptLineHighlightMode::Solid => Self::Solid,
            ScriptLineHighlightMode::Gradient(opacity) => Self::Gradient(opacity),
        }
    }
}

impl ScriptRepr for LineHighlightMode {
    type Repr = ScriptLineHighlightMode;

    fn into_repr(self) -> Self::Repr {
        self.into()
    }

    fn from_repr(repr: Self::Repr) -> Self {
        repr.into()
    }
}

/// Registers the script-facing names for [`LineHighlightMode`].
pub(super) fn base_module() -> Result<rune::Module, rune::ContextError> {
    let mut m = rune::Module::new();
    m.ty::<ScriptLineHighlightMode>()?;
    Ok(m)
}
