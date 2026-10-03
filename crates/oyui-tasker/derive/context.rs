//! `TaskerContext` derive: composes a listener context from field-wise extractions.

use proc_macro::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Generates a blanket `ExtractsFrom<C>` impl delegating to each field type.
pub(crate) fn expand(input: DeriveInput) -> TokenStream {
    let name = &input.ident;

    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => &fields.named,
            _ => panic!("TaskerContext only supports structs with named fields"),
        },
        _ => panic!("TaskerContext only supports structs"),
    };

    let field_names = fields.iter().map(|f| &f.ident);
    let field_types = fields.iter().map(|f| &f.ty);
    let field_types_2 = fields.iter().map(|f| &f.ty);

    let expanded = quote! {
        impl<__C> ::oyui_tasker::worker::ExtractsFrom<__C> for #name
        where
            #( #field_types: ::oyui_tasker::worker::ExtractsFrom<__C> ),*
        {
            fn extract(ctx: &__C) -> Self {
                Self {
                    #(
                        #field_names: <#field_types_2 as ::oyui_tasker::worker::ExtractsFrom<__C>>::extract(ctx)
                    ),*
                }
            }
        }
    };

    TokenStream::from(expanded)
}
