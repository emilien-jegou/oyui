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

/// `Option<T>` mirrors the engine's own option type, so absence crosses the
/// boundary as `Some`/`None` instead of a lossy sentinel value.
impl<T> ScriptRepr for Option<T>
where
    T: ScriptRepr,
{
    type Repr = Option<T::Repr>;

    fn into_repr(self) -> Self::Repr {
        self.map(T::into_repr)
    }

    fn from_repr(repr: Self::Repr) -> Self {
        repr.map(T::from_repr)
    }
}

pub mod reexport {
    pub use rune;
}

#[cfg(test)]
mod tests {
    use super::ScriptRepr;

    #[test]
    fn option_repr_round_trips() {
        assert_eq!(<Option<u32> as ScriptRepr>::into_repr(Some(3)), Some(3));
        assert_eq!(<Option<u32> as ScriptRepr>::from_repr(None), None);
        assert_eq!(
            <Option<String> as ScriptRepr>::into_repr(Some("x".into())),
            Some("x".to_string())
        );
    }
}
