pub use oyui_rune_actions_derive::*;

#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)]
pub enum ActionsGetSet<T> {
    set(T),
    get,
}

pub mod reexport {
    pub use rune;
}
