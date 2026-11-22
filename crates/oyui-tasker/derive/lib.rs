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
///
/// Apply this to a struct with named fields. Each field type must be `Clone`;
/// extraction clones the corresponding field from the registry context. Tuple
/// structs, unit structs, enums, and unions are not supported. Generic context
/// structs are not currently supported.
///
/// Pair with `#[derive(TaskerContext)]` on listener context structs when a
/// listener needs several fields:
///
/// ```ignore
/// #[derive(Clone, TaskerProvide)]
/// struct AppContext { multiplier: i32 }
///
/// #[derive(TaskerContext)]
/// struct MathContext { multiplier: i32 }
/// ```
#[proc_macro_derive(TaskerProvide)]
pub fn derive_tasker_provide(input: TokenStream) -> TokenStream {
    provide::expand(parse_macro_input!(input as DeriveInput))
}

/// Derives a composable `ExtractsFrom` impl for a listener context struct.
///
/// Apply this to a struct with named fields. Each field type must implement
/// `ExtractsFrom<RegistryContext>`; extraction composes those field
/// extractions to build the listener context. Tuple structs, unit structs,
/// enums, and unions are not supported. Generic context structs are not
/// currently supported.
#[proc_macro_derive(TaskerContext)]
pub fn derive_tasker_context(input: TokenStream) -> TokenStream {
    context::expand(parse_macro_input!(input as DeriveInput))
}

/// Generates the event enum, sender/receiver, and dispatch loop of a registry.
///
/// `events` declares variant names and payload types. `listeners` maps variants
/// to listener types. Optional `internal` variants are dispatched to listeners
/// but omitted from the mirrored receiver; optional `collapse` variants are
/// mirrored with newest-value-wins coalescing. Optional `replies` pairs request
/// variants with response types for `EventRegistry::ask`.
///
/// The generated registry can be consumed as one owner (`spawn`, `send`,
/// `recv`, `shutdown`) or split with `into_split` into a sender, receiver, and
/// optional Tokio join handle. A split registry cannot call `ask`; the
/// returned sender can still send events and request shutdown. When splitting,
/// send `shutdown()` and await the returned handle if present to finish the
/// dispatch task.
#[proc_macro]
pub fn tasker_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RegistryInput);

    if let Err(err) = input.validate() {
        return err.to_compile_error().into();
    }

    registry::expand(&input)
}
