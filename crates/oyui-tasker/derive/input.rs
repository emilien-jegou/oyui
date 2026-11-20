//! Parses and validates the `tasker_registry!` DSL (`events` and `listeners` lists).

use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
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

/// One `RequestEvent => ResponseType` entry of `replies = [...]`.
pub(crate) struct ReplyDef {
    pub(crate) event_name: Ident,
    pub(crate) response: Type,
}

/// The declaration lists of `tasker_registry!`.
pub(crate) struct RegistryInput {
    pub(crate) events: Vec<EventDef>,
    pub(crate) listeners: Vec<ListenerDef>,
    pub(crate) replies: Vec<ReplyDef>,
    /// Events that never reach the app-facing receiver: plumbing only.
    pub(crate) internal: Vec<Ident>,
    /// Events whose bursts are coalesced to one pending copy (latest wins).
    pub(crate) collapse: Vec<Ident>,
}

impl RegistryInput {
    /// Rejects listeners bound to events missing from `events = [...]`.
    fn validate_listeners(&self) -> syn::Result<()> {
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

    /// Rejects routing classes naming events missing from `events = [...]`.
    fn validate_classes(&self) -> syn::Result<()> {
        for (list, what) in [(&self.internal, "internal"), (&self.collapse, "collapse")] {
            let mut seen: Vec<String> = Vec::new();
            for name in list {
                if !self.events.iter().any(|e| e.name == *name) {
                    return Err(syn::Error::new(
                        name.span(),
                        format!(
                            "`{name}` is not declared in `events = [...]` \
                             (cannot be marked `{what}`)",
                        ),
                    ));
                }
                if seen.iter().any(|other| other == &name.to_string()) {
                    return Err(syn::Error::new(
                        name.span(),
                        format!("`{name}` is listed twice in `{what}`"),
                    ));
                }
                seen.push(name.to_string());
            }
        }
        Ok(())
    }

    /// Rejects a reply declared for an unknown event, and an event that claims
    /// to be asked but is not declared with an `Asked<...>` payload.
    fn validate_replies(&self) -> syn::Result<()> {
        for reply in &self.replies {
            let Some(event) = self.events.iter().find(|e| e.name == reply.event_name) else {
                return Err(syn::Error::new(
                    reply.event_name.span(),
                    format!(
                        "`{}` is not declared in `events = [...]` \
                         (cannot declare a reply for it)",
                        reply.event_name
                    ),
                ));
            };

            // The registry routes a reply by the token carried in the payload,
            // so the payload must be an `Asked<...>` wrapper for there to be one.
            let is_asked = match &event.ty {
                syn::Type::Path(path) => path
                    .path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "Asked"),
                _ => false,
            };
            if !is_asked {
                return Err(syn::Error::new(
                    event.ty.span(),
                    format!(
                        "`{}` must carry an `Asked<...>` payload to be asked; \
                         its listeners receive the request by value and reply \
                         through the sender instead",
                        reply.event_name
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Rejects an event that is both invisible to the app and coalesced:
    /// coalescing only decides how a mirrored burst is collapsed.
    fn validate_overlap(&self) -> syn::Result<()> {
        for name in &self.collapse {
            if self.internal.iter().any(|other| other == name) {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "`{name}` is both `internal` and `collapse`; \
                         an internal event is never mirrored, so there is \
                         nothing to coalesce"
                    ),
                ));
            }
        }
        Ok(())
    }

    /// Runs every validation pass over the parsed declaration lists.
    pub(crate) fn validate(&self) -> syn::Result<()> {
        self.validate_listeners()?;
        self.validate_classes()?;
        self.validate_replies()?;
        self.validate_overlap()
    }
}

impl Parse for RegistryInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut events = Vec::new();
        let mut listeners = Vec::new();
        let mut replies = Vec::new();
        let mut internal = Vec::new();
        let mut collapse = Vec::new();

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
            } else if key == "internal" || key == "collapse" {
                let content;
                bracketed!(content in input);
                let target = if key == "internal" {
                    &mut internal
                } else {
                    &mut collapse
                };
                while !content.is_empty() {
                    target.push(content.parse::<Ident>()?);
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            } else if key == "replies" {
                let content;
                bracketed!(content in input);
                while !content.is_empty() {
                    let event_name: Ident = content.parse()?;
                    content.parse::<Token![=>]>()?;
                    let response: Type = content.parse()?;
                    replies.push(ReplyDef {
                        event_name,
                        response,
                    });
                    if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                    }
                }
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected 'events', 'listeners', 'replies', 'internal' or 'collapse'",
                ));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(RegistryInput {
            events,
            listeners,
            replies,
            internal,
            collapse,
        })
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

    /// `internal` and `collapse` must name declared events, or the generated
    /// arms would silently never match.
    #[test]
    fn rejects_routing_class_for_undeclared_event() {
        for source in [
            "events = [Ping => u8], internal = [Typo]",
            "events = [Ping => u8], collapse = [Typo]",
        ] {
            let input: RegistryInput = syn::parse_str(source).expect("input parses");
            let err = input
                .validate()
                .expect_err("unknown routing target must be rejected");
            assert!(err.to_string().contains("Typo"), "got: {err}");
        }
    }

    /// An event that is never mirrored has no burst to coalesce.
    #[test]
    fn rejects_event_marked_both_internal_and_collapse() {
        let input: RegistryInput =
            syn::parse_str("events = [Ping => u8], internal = [Ping], collapse = [Ping]")
                .expect("input parses");

        let err = input
            .validate()
            .expect_err("overlapping routing classes must be rejected");
        assert!(err.to_string().contains("Ping"), "got: {err}");
    }

    #[test]
    fn accepts_declared_routing_classes() {
        let input: RegistryInput = syn::parse_str(
            "events = [Ping => u8, Pong => u8], \
             internal = [Ping], collapse = [Pong], \
             listeners = [Ping => [SomeListener]]",
        )
        .expect("input parses");

        input.validate().expect("declared routes are accepted");
        assert_eq!(input.internal.len(), 1);
        assert_eq!(input.collapse.len(), 1);
    }

    /// A reply must point at a declared event, or the pairing is dead code.
    #[test]
    fn rejects_reply_for_undeclared_event() {
        let input: RegistryInput =
            syn::parse_str("events = [Ping => u8], replies = [Nope => u8]").expect("input parses");

        let err = input
            .validate()
            .expect_err("unknown reply target must be rejected");
        assert!(err.to_string().contains("Nope"), "got: {err}");
    }

    /// The reply is routed by the token in the payload, so the payload has to be
    /// an `Asked<...>` for a token to exist at all.
    #[test]
    fn rejects_reply_for_event_without_an_asked_payload() {
        let input: RegistryInput =
            syn::parse_str("events = [Ping => u8], replies = [Ping => u8]").expect("input parses");

        let err = input
            .validate()
            .expect_err("a plain payload cannot be asked");
        assert!(err.to_string().contains("Asked"), "got: {err}");
    }

    #[test]
    fn accepts_a_declared_reply() {
        let input: RegistryInput = syn::parse_str(
            "events = [Ask => Asked<Question>, Answer => Answer], \
             replies = [Ask => Answer], \
             listeners = [Ask => [Answerer]]",
        )
        .expect("input parses");

        input.validate().expect("declared reply is accepted");
        assert_eq!(input.replies.len(), 1);
    }
}
