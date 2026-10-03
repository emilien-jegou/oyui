//! Parses and validates the `tasker_registry!` DSL (`events` and `listeners` lists).

use syn::parse::{Parse, ParseStream};
use syn::{bracketed, Ident, Token, Type};

/// One `Variant => PayloadType` entry of `events = [...]`.
pub(crate) struct EventDef {
    pub(crate) name: Ident,
    pub(crate) ty: Type,
}

/// One `Event => [Listener, ...]` entry of `listeners = [...]`.
pub(crate) struct ListenerDef {
    pub(crate) event_name: Ident,
    pub(crate) listeners: Vec<Type>,
}

/// The two declaration lists of `tasker_registry!`.
pub(crate) struct RegistryInput {
    pub(crate) events: Vec<EventDef>,
    pub(crate) listeners: Vec<ListenerDef>,
}

impl RegistryInput {
    /// Rejects listeners bound to events missing from `events = [...]`.
    pub(crate) fn validate(&self) -> syn::Result<()> {
        for listener in &self.listeners {
            let known = self.events.iter().any(|e| e.name == listener.event_name);
            if !known {
                return Err(syn::Error::new(
                    listener.event_name.span(),
                    format!(
                        "`{}` is not declared in `events = [...]` \
                         (listeners for unknown events are never invoked)",
                        listener.event_name
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl Parse for RegistryInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut events = Vec::new();
        let mut listeners = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if key == "events" {
                let content;
                bracketed!(content in input);
                while !content.is_empty() {
                    let name: Ident = content.parse()?;
                    content.parse::<Token![=>]>()?;
                    let ty: Type = content.parse()?;
                    events.push(EventDef { name, ty });
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            } else if key == "listeners" {
                let content;
                bracketed!(content in input);
                while !content.is_empty() {
                    let event_name: Ident = content.parse()?;
                    content.parse::<Token![=>]>()?;

                    let list_content;
                    bracketed!(list_content in content);
                    let mut list = Vec::new();
                    while !list_content.is_empty() {
                        let listener_ty: Type = list_content.parse()?;
                        list.push(listener_ty);
                        if list_content.peek(Token![,]) {
                            list_content.parse::<Token![,]>()?;
                        }
                    }
                    listeners.push(ListenerDef {
                        event_name,
                        listeners: list,
                    });

                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected 'events' or 'listeners'",
                ));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(RegistryInput { events, listeners })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_listener_for_undeclared_event() {
        let input: RegistryInput =
            syn::parse_str("events = [Ping => u8], listeners = [Typo => [SomeListener]]")
                .expect("input parses");

        let err = input
            .validate()
            .expect_err("listener for an undeclared event must be rejected");
        assert!(err.to_string().contains("Typo"));
    }

    #[test]
    fn accepts_listener_for_declared_event() {
        let input: RegistryInput =
            syn::parse_str("events = [Ping => u8], listeners = [Ping => [SomeListener]]")
                .expect("input parses");

        input.validate().expect("declared events are accepted");
    }
}
