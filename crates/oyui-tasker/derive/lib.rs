use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(TaskerProvide)]
pub fn derive_tasker_provide(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
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
            // "The FieldType knows how to extract itself from AppWorkerContext"
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

#[proc_macro_derive(TaskerContext)]
pub fn derive_tasker_context(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
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
