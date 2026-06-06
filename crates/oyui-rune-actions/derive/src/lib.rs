extern crate proc_macro;

use heck::ToPascalCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    braced, parenthesized,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    Ident, Result, Token, Type,
};

struct ActionTree {
    root_nodes: Vec<ActionNode>,
}

enum ActionNode {
    Branch {
        ident: Ident,
        getset: Option<Type>,
        children: Vec<ActionNode>,
    },
    Leaf {
        ident: Ident,
        args: Vec<Type>,
        ret_type: Type,
    },
}

struct LeafInfo {
    ident: Ident,
    args: Vec<Type>,
    ret_type: Type,
}

impl Parse for ActionTree {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut root_nodes = Vec::new();
        while !input.is_empty() {
            root_nodes.push(input.parse()?);
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(ActionTree { root_nodes })
    }
}

impl Parse for ActionNode {
    fn parse(input: ParseStream) -> Result<Self> {
        let ident: Ident = input.parse()?;

        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            let mut getset = None;
            let mut children = Vec::new();

            while !content.is_empty() {
                if content.peek(Token![@]) {
                    content.parse::<Token![@]>()?;
                    let kw: Ident = content.parse()?;
                    if kw == "getset" {
                        let inner;
                        parenthesized!(inner in content);
                        getset = Some(inner.parse()?);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    } else {
                        return Err(syn::Error::new(kw.span(), "expected `getset`"));
                    }
                } else {
                    children.push(content.parse()?);
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            }
            Ok(ActionNode::Branch {
                ident,
                getset,
                children,
            })
        } else {
            let content;
            parenthesized!(content in input);
            let mut args = Vec::new();
            let mut ret_type: Type = syn::parse_quote!(());

            if content.peek(Token![|]) {
                content.parse::<Token![|]>()?;
                while !content.peek(Token![|]) && !content.is_empty() {
                    args.push(content.parse()?);
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    } else {
                        break;
                    }
                }
                content.parse::<Token![|]>()?;
                if content.peek(Token![->]) {
                    content.parse::<Token![->]>()?;
                    ret_type = content.parse()?;
                }
            } else {
                let punctuated: Punctuated<Type, Token![,]> =
                    content.parse_terminated(Type::parse, Token![,])?;
                args = punctuated.into_iter().collect();
            }
            Ok(ActionNode::Leaf {
                ident,
                args,
                ret_type,
            })
        }
    }
}

#[proc_macro]
pub fn define_actions(input: TokenStream) -> TokenStream {
    let tree = syn::parse_macro_input!(input as ActionTree);

    // Identify active branches (those containing direct leaf actions or getsets)
    let mut active_branches = Vec::new();
    for node in &tree.root_nodes {
        let mut path = vec![node.ident().clone()];
        find_active_branches(node, &mut path, &mut active_branches);
    }

    // Generate pure structural Rust Enums
    let mut enum_definitions = Vec::new();
    let root_variants: Vec<TokenStream2> = tree
        .root_nodes
        .iter()
        .map(|node| {
            let node_ident = node.ident();
            let enum_name = format_ident!("{}Actions", pascal_case(node_ident));
            quote! { #node_ident(#enum_name) }
        })
        .collect();

    enum_definitions.push(quote! {
        #[derive(Debug, Clone, PartialEq)]
        #[allow(non_camel_case_types, dead_code)]
        pub enum Actions {
            #(#root_variants),*
        }
    });

    for node in &tree.root_nodes {
        let mut path_idents = vec![node.ident().clone()];
        generate_nodes_grouped(node, &mut path_idents, &mut enum_definitions);
    }

    // Generate automatic `From` traits mappings into Action wrapper
    let mut from_impls = Vec::new();
    for node in &tree.root_nodes {
        let mut path_idents = vec![node.ident().clone()];
        generate_from_impls(node, &mut path_idents, &mut from_impls);
    }

    // Generate leaf Traits (including Arc delegation implementation)
    let mut traits = Vec::new();
    for (path, getset, leaves) in &active_branches {
        let trait_name = path_to_trait_name(path);
        let mut trait_methods = Vec::new();
        let mut arc_methods = Vec::new();

        for leaf in leaves {
            let leaf_ident = &leaf.ident;
            let args = &leaf.args;
            let ret_type = &leaf.ret_type;
            let arg_names: Vec<Ident> = (0..args.len()).map(|i| format_ident!("a{}", i)).collect();

            trait_methods.push(quote! {
                fn #leaf_ident(&self, #(#arg_names: #args),*) -> #ret_type;
            });

            arc_methods.push(quote! {
                fn #leaf_ident(&self, #(#arg_names: #args),*) -> #ret_type {
                    (**self).#leaf_ident(#(#arg_names),*)
                }
            });
        }

        if let Some(ty) = getset {
            trait_methods.push(quote! {
                fn get(&self) -> #ty;
                fn set(&self, val: #ty);
            });

            arc_methods.push(quote! {
                fn get(&self) -> #ty {
                    (**self).get()
                }
                fn set(&self, val: #ty) {
                    (**self).set(val)
                }
            });
        }

        traits.push(quote! {
            pub trait #trait_name {
                #(#trait_methods)*
            }

            impl<T: #trait_name + ?Sized> #trait_name for ::std::sync::Arc<T> {
                #(#arc_methods)*
            }
        });
    }

    // Generate the flat Handler and non-generic BoxedHandler structs
    let mut fields = Vec::new();
    let mut field_names = Vec::new();
    let mut generic_params = Vec::new();
    let mut trait_bounds = Vec::new();
    let mut erased_types = Vec::new();
    let mut erased_instantiations = Vec::new();

    for (path, _, _) in &active_branches {
        let field_name = path_to_field_name(path);
        let generic_param = path_to_generic_param(path);
        let trait_name = path_to_trait_name(path);

        fields.push(quote! {
            pub #field_name: #generic_param
        });
        field_names.push(field_name.clone());
        generic_params.push(generic_param.clone());
        trait_bounds.push(quote! {
            #generic_param: #trait_name + Send + Sync + 'static
        });
        erased_types.push(quote! {
            Box<dyn #trait_name + Send + Sync + 'static>
        });
        erased_instantiations.push(quote! {
            #field_name: Box::new(self.#field_name) as Box<dyn #trait_name + Send + Sync + 'static>
        });
    }

    // 5.5 Generate dispatch method implementation arms for BoxedHandler
    let mut dispatch_arms = Vec::new();
    for (path, _getset, leaves) in &active_branches {
        let field_name = path_to_field_name(path);
        let deepest_enum_name = build_enum_name(path);

        for leaf in leaves {
            let leaf_ident = &leaf.ident;
            let args = &leaf.args;
            let arg_names: Vec<Ident> = (0..args.len()).map(|i| format_ident!("a{}", i)).collect();

            // Setup the inner variant matcher
            let mut pattern = if args.is_empty() {
                quote! { #deepest_enum_name::#leaf_ident }
            } else {
                quote! { #deepest_enum_name::#leaf_ident(#(#arg_names),*) }
            };

            // Recursively construct the pattern up to the root enum
            for j in (0..path.len()).rev() {
                let variant = &path[j];
                let enum_name = if j == 0 {
                    quote! { Actions }
                } else {
                    let name = build_enum_name(&path[0..j]);
                    quote! { #name }
                };
                pattern = quote! { #enum_name::#variant(#pattern) };
            }

            dispatch_arms.push(quote! {
                #pattern => {
                    let _ = self.0.#field_name.#leaf_ident(#(#arg_names.clone()),*);
                }
            });
        }
    }

    let handler_struct = quote! {
        #[derive(Clone, Debug)]
        pub struct Handler<#(#generic_params),*> {
            #(#fields),*
        }

        // BoxedHandler is now fully type-erased (no generic parameters)
        #[derive(Clone)]
        pub struct BoxedHandler(
            pub ::std::sync::Arc<Handler<#(#erased_types),*>>
        );

        impl BoxedHandler {
            pub fn dispatch(&self, action: &Action) {
                match &action.0 {
                    #(#dispatch_arms,)*
                    _ => {} // Covers Unhandled __GetSet variants
                }
            }
        }

        impl<#(#generic_params),*> Handler<#(#generic_params),*>
        where
            #(#trait_bounds),*
        {
            pub fn build(self) -> BoxedHandler {
                BoxedHandler(::std::sync::Arc::new(Handler {
                    #(#erased_instantiations),*
                }))
            }
        }
    };

    // Generate registrations grouped by namespace path
    let mut module_registrations =
        std::collections::HashMap::<Vec<String>, Vec<TokenStream2>>::new();

    for (path, getset, leaves) in &active_branches {
        let string_path: Vec<String> = path
            .iter()
            .map(|id| id.to_string().to_lowercase())
            .collect();
        let field_name = path_to_field_name(path);

        module_registrations.entry(string_path.clone()).or_default();

        for leaf in leaves {
            let leaf_ident = &leaf.ident;
            let leaf_name = leaf_ident.to_string();
            let args = &leaf.args;
            let ret_type = &leaf.ret_type;
            let arg_names: Vec<Ident> = (0..args.len()).map(|i| format_ident!("a{}", i)).collect();

            let reg = quote! {
                {
                    let handler_clone = handler.0.clone();
                    let func = move |#(#arg_names: #args),*| -> #ret_type {
                        handler_clone.#field_name.#leaf_ident(#(#arg_names),*)
                    };
                    m.function(#leaf_name, func).build()?;
                }
            };

            module_registrations
                .get_mut(&string_path)
                .unwrap()
                .push(reg);
        }

        if let Some(ty) = getset {
            let reg_set = quote! {
                {
                    let handler_clone = handler.0.clone();
                    let func = move |a0: #ty| {
                        handler_clone.#field_name.set(a0);
                    };
                    m.function("set", func).build()?;
                }
            };
            let reg_get = quote! {
                {
                    let handler_clone = handler.0.clone();
                    let func = move || -> #ty {
                        handler_clone.#field_name.get()
                    };
                    m.function("get", func).build()?;
                }
            };

            let regs = module_registrations.get_mut(&string_path).unwrap();
            regs.push(reg_set);
            regs.push(reg_get);
        }
    }

    let mut keys: Vec<Vec<String>> = module_registrations.keys().cloned().collect();
    keys.sort_by_key(|k| k.len());

    let mut rune_registrations = Vec::new();
    for path in keys {
        let regs = &module_registrations[&path];
        rune_registrations.push(quote! {
            {
                let mut m = ::oyui_rune_actions::reexport::rune::Module::with_item(&[#(#path),*])?;
                #(#regs)*
                context.install(&m)?;
            }
        });
    }

    let expanded = quote! {
        #(#enum_definitions)*

        #(#traits)*

        #handler_struct

        #[derive(Clone, Debug, ::oyui_rune_actions::reexport::rune::Any)]
        pub struct Action(pub Actions);

        impl From<Actions> for Action {
            fn from(val: Actions) -> Self {
                Action(val)
            }
        }

        #(#from_impls)*

        pub fn register_actions(
            context: &mut ::oyui_rune_actions::reexport::rune::Context,
            handler: BoxedHandler,
        ) -> Result<(), ::oyui_rune_actions::reexport::rune::ContextError>
        {
            #(#rune_registrations)*
            Ok(())
        }
    };

    TokenStream::from(expanded)
}

fn find_active_branches(
    node: &ActionNode,
    current_path: &mut Vec<Ident>,
    active: &mut Vec<(Vec<Ident>, Option<Type>, Vec<LeafInfo>)>,
) {
    match node {
        ActionNode::Branch {
            getset, children, ..
        } => {
            let mut leaves = Vec::new();
            let mut branches = Vec::new();
            for child in children {
                match child {
                    ActionNode::Leaf {
                        ident,
                        args,
                        ret_type,
                    } => {
                        leaves.push(LeafInfo {
                            ident: ident.clone(),
                            args: args.clone(),
                            ret_type: ret_type.clone(),
                        });
                    }
                    ActionNode::Branch { .. } => {
                        branches.push(child);
                    }
                }
            }

            if !leaves.is_empty() || getset.is_some() {
                active.push((current_path.clone(), getset.clone(), leaves));
            }

            for branch in branches {
                current_path.push(branch.ident().clone());
                find_active_branches(branch, current_path, active);
                current_path.pop();
            }
        }
        ActionNode::Leaf { .. } => {}
    }
}

fn generate_nodes_grouped(
    node: &ActionNode,
    current_path: &mut Vec<Ident>,
    enum_defs: &mut Vec<TokenStream2>,
) {
    match node {
        ActionNode::Branch {
            ident: _,
            getset,
            children,
        } => {
            let current_enum_name = build_enum_name(current_path);

            let mut variants: Vec<TokenStream2> = children
                .iter()
                .map(|child| {
                    let child_ident = child.ident();
                    match child {
                        ActionNode::Branch { .. } => {
                            current_path.push(child_ident.clone());
                            let child_enum_name = build_enum_name(current_path);
                            current_path.pop();
                            quote! { #child_ident(#child_enum_name) }
                        }
                        ActionNode::Leaf { args, .. } => {
                            if args.is_empty() {
                                quote! { #child_ident }
                            } else {
                                quote! { #child_ident(#(#args),*) }
                            }
                        }
                    }
                })
                .collect();

            if let Some(ty) = getset {
                variants.push(quote! { __GetSet(::oyui_rune_actions::ActionsGetSet<#ty>) });
            }

            enum_defs.push(quote! {
                #[derive(Debug, Clone, PartialEq)]
                #[allow(non_camel_case_types, dead_code)]
                pub enum #current_enum_name {
                    #(#variants),*
                }
            });

            for child in children {
                if let ActionNode::Branch { .. } = child {
                    current_path.push(child.ident().clone());
                    generate_nodes_grouped(child, current_path, enum_defs);
                    current_path.pop();
                }
            }
        }
        ActionNode::Leaf { .. } => {}
    }
}

fn generate_from_impls(
    node: &ActionNode,
    current_path: &mut Vec<Ident>,
    from_impls: &mut Vec<TokenStream2>,
) {
    match node {
        ActionNode::Branch { children, .. } => {
            let current_enum_name = build_enum_name(current_path);

            let mut wrap_expr = quote! { val };
            for j in (0..current_path.len()).rev() {
                let variant_ident = &current_path[j];
                let parent_enum = if j == 0 {
                    quote! { Actions }
                } else {
                    let parent_name = build_enum_name(&current_path[0..j]);
                    quote! { #parent_name }
                };
                wrap_expr = quote! { #parent_enum::#variant_ident(#wrap_expr) };
            }

            from_impls.push(quote! {
                impl From<#current_enum_name> for Action {
                    fn from(val: #current_enum_name) -> Self {
                        Action(#wrap_expr)
                    }
                }
            });

            for child in children {
                if let ActionNode::Branch { .. } = child {
                    current_path.push(child.ident().clone());
                    generate_from_impls(child, current_path, from_impls);
                    current_path.pop();
                }
            }
        }
        ActionNode::Leaf { .. } => {}
    }
}

fn path_to_field_name(path: &[Ident]) -> Ident {
    let parts: Vec<String> = path
        .iter()
        .map(|id| id.to_string().to_lowercase())
        .collect();
    format_ident!("{}", parts.join("_"))
}

fn path_to_generic_param(path: &[Ident]) -> Ident {
    let mut name = String::new();
    for id in path {
        name.push_str(&pascal_case(id));
    }
    name.push('T');
    format_ident!("{}", name)
}

fn path_to_trait_name(path: &[Ident]) -> Ident {
    let mut name = String::new();
    for id in path {
        name.push_str(&pascal_case(id));
    }
    name.push_str("ActionsHandler");
    format_ident!("{}", name)
}

fn build_enum_name(path: &[Ident]) -> Ident {
    let mut name = String::new();
    for ident in path {
        name.push_str(&pascal_case(ident));
    }
    name.push_str("Actions");
    format_ident!("{}", name)
}

fn pascal_case(ident: &Ident) -> String {
    ident.to_string().to_pascal_case()
}

impl ActionNode {
    fn ident(&self) -> &Ident {
        match self {
            ActionNode::Branch { ident, .. } => ident,
            ActionNode::Leaf { ident, .. } => ident,
        }
    }
}
