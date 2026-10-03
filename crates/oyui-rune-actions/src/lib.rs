//! Rune-free runtime surface of the action framework: the get/set carrier and
//! the [`ScriptRepr`] bridge that lets a script host marshal values across the
//! language boundary without the core action types knowing about any engine.

pub use oyui_rune_actions_derive::*;

#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum ActionsGetSet<T> {
    set(T),
    get,
}

/// Engine-side representation of a core type when it crosses into a script.
///
/// The core type stays engine-agnostic; the host crate supplies the concrete
/// representation its engine uses. Primitive types default to themselves.
pub trait ScriptRepr: Sized {
    type Repr;
    fn into_repr(self) -> Self::Repr;
    fn from_repr(repr: Self::Repr) -> Self;
}

macro_rules! impl_script_repr_identity {
    ($($ty:ty),* $(,)?) => {
        $(impl ScriptRepr for $ty {
            type Repr = $ty;
            fn into_repr(self) -> Self::Repr { self }
            fn from_repr(repr: Self::Repr) -> Self { repr }
        })*
    };
}

impl_script_repr_identity!(bool, String, u32, u64, f64, ());

pub mod reexport {
    pub use rune;
}
