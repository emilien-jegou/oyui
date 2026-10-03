//! Proc-macro entry points: context derives and the `tasker_registry!` generator.

mod context;
mod input;
mod provide;
mod registry;

use proc_macro::TokenStream;
use syn::parse_macro_input;
use syn::DeriveInput;

use crate::input::RegistryInput;

/// Derives per-field `ExtractsFrom` impls so listeners can pull context pieces.
#[proc_macro_derive(TaskerProvide)]
pub fn derive_tasker_provide(input: TokenStream) -> TokenStream {
    provide::expand(parse_macro_input!(input as DeriveInput))
}

/// Derives a composable `ExtractsFrom` impl for a listener context struct.
#[proc_macro_derive(TaskerContext)]
pub fn derive_tasker_context(input: TokenStream) -> TokenStream {
    context::expand(parse_macro_input!(input as DeriveInput))
}

/// Generates the event enum, sender/receiver, and dispatch loop of a registry.
#[proc_macro]
pub fn tasker_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RegistryInput);

    if let Err(err) = input.validate() {
        return err.to_compile_error().into();
    }

    registry::expand(&input)
}
