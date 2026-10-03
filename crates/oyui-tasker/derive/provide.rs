//! `TaskerProvide` derive: clones each field of a global context struct out for listeners.

use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Generates one `ExtractsFrom<Ctx>` impl per field of the annotated struct.
pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    let struct_name = &input.ident;

    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => &fields.named,
            _ => panic!("TaskerProvide only supports structs with named fields"),
        },
        _ => panic!("TaskerProvide only supports structs"),
    };

    let expanded = fields.iter().map(|f| {
        let field_name = &f.ident;
        let field_ty = &f.ty;

        quote! {
            impl ::oyui_tasker::worker::ExtractsFrom<#struct_name> for #field_ty {
                fn extract(ctx: &#struct_name) -> Self {
                    ctx.#field_name.clone()
                }
            }
        }
    });

    TokenStream::from(quote! {
        #(#expanded)*
    })
}
