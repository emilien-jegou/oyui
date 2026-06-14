use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{bracketed, parse_macro_input, DeriveInput, Ident, Token, Type};

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

struct EventDef {
    name: Ident,
    ty: Type,
}

struct ListenerDef {
    event_name: Ident,
    listeners: Vec<Type>,
}

struct RegistryInput {
    events: Vec<EventDef>,
    listeners: Vec<ListenerDef>,
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

#[proc_macro]
pub fn tasker_registry(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as RegistryInput);

    let mut mapped_events = Vec::new();
    for event in &input.events {
        let event_name = &event.name;
        let event_ty = &event.ty;

        let event_listeners = input
            .listeners
            .iter()
            .find(|l| l.event_name == *event_name)
            .map(|l| &l.listeners)
            .cloned()
            .unwrap_or_default();

        mapped_events.push((event_name, event_ty, event_listeners));
    }

    let enum_variants = mapped_events.iter().map(|(name, ty, _)| {
        quote! { #name(#ty) }
    });

    let from_impls = mapped_events.iter().map(|(name, ty, _)| {
        quote! {
            impl From<#ty> for Event {
                fn from(ev: #ty) -> Self {
                    Event::#name(ev)
                }
            }
        }
    });

    let spawn_bounds = mapped_events.iter().flat_map(|(_, ty, listeners)| {
        listeners.iter().map(move |listener_ty| {
            quote! {
                <#listener_ty as ::oyui_tasker::worker::Listener<#ty, EventSender>>::Context: ::oyui_tasker::worker::ExtractsFrom<C>
            }
        })
    });

    let match_arms = mapped_events.iter().map(|(name, ty, listeners)| {
        let listener_spawns = listeners.iter().map(|listener_ty| {
            quote! {
                let ctx_extracted = <<#listener_ty as ::oyui_tasker::worker::Listener<#ty, EventSender>>::Context as ::oyui_tasker::worker::ExtractsFrom<C>>::extract(&*c);
                let ev_clone = ev.clone();
                let tx = tx_clone.clone();

                ::tokio::spawn(
                    async move {
                        let span = ::oyui_tasker::reexport::tracing::info_span!(
                            "listener_handle",
                            event_type = stringify!(#name),
                            listener = stringify!(#listener_ty)
                        );

                        use ::oyui_tasker::reexport::tracing::Instrument;
                        async {
                            let res = <#listener_ty as ::oyui_tasker::worker::Listener<#ty, EventSender>>::handle(ev_clone, ctx_extracted, tx).await;
                            match res {
                                Ok(_) => {
                                    ::oyui_tasker::reexport::tracing::trace!("Listener completed successfully");
                                }
                                Err(e) => {
                                    ::oyui_tasker::reexport::tracing::error!(error = ?e, "Listener failed");
                                }
                            }
                        }
                        .instrument(span)
                        .await
                    }
                );
            }
        });

        quote! {
            Event::#name(ev) => {
                let _ = ev_tx.try_send(Event::#name(ev.clone()));
                #( #listener_spawns )*
            }
        }
    });

    let expanded = quote! {
        #[derive(Debug, Clone)]
        pub enum Event {
            #( #enum_variants, )*
            Shutdown,
        }

        #( #from_impls )*

        #[derive(Clone)]
        pub struct EventSender {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
        }

        impl EventSender {
            pub fn send<E>(&self, event: E) -> Result<(), ::tokio::sync::mpsc::error::SendError<Event>>
            where
                Event: From<E>,
            {
                let ev = Event::from(event);
                ::oyui_tasker::reexport::tracing::debug!(?ev, "EventSender sending event");
                self.tx.try_send(ev).map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TrySendError::Full(_) => unreachable!("unbounded channel cannot be full"),
                    ::oyui_tasker::reexport::async_channel::TrySendError::Closed(val) => ::tokio::sync::mpsc::error::SendError(val),
                })
            }

            pub fn shutdown(&self) -> Result<(), ::tokio::sync::mpsc::error::SendError<Event>> {
                ::oyui_tasker::reexport::tracing::info!("EventSender sending Shutdown signal");
                self.tx.try_send(Event::Shutdown).map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TrySendError::Full(_) => unreachable!("unbounded channel cannot be full"),
                    ::oyui_tasker::reexport::async_channel::TrySendError::Closed(val) => ::tokio::sync::mpsc::error::SendError(val),
                })
            }
        }

        pub struct EventReceiver {
            rx: ::oyui_tasker::reexport::async_channel::Receiver<Event>,
        }

        impl EventReceiver {
            pub async fn recv(&self) -> Option<Event> {
                self.rx.recv().await.ok()
            }

            pub fn try_recv(&self) -> Result<Event, ::tokio::sync::mpsc::error::TryRecvError> {
                self.rx.try_recv().map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TryRecvError::Empty => ::tokio::sync::mpsc::error::TryRecvError::Empty,
                    ::oyui_tasker::reexport::async_channel::TryRecvError::Closed => ::tokio::sync::mpsc::error::TryRecvError::Disconnected,
                })
            }
        }

        pub struct EventRegistry {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
            rx: ::oyui_tasker::reexport::async_channel::Receiver<Event>,
            handle: ::std::sync::Mutex<Option<::tokio::task::JoinHandle<()>>>,
        }

        impl EventRegistry {
            pub fn sender(&self) -> EventSender {
                EventSender { tx: self.tx.clone() }
            }

            pub fn send<E>(&self, event: E) -> Result<(), ::tokio::sync::mpsc::error::SendError<Event>>
            where
                Event: From<E>,
            {
                let ev = Event::from(event);
                ::oyui_tasker::reexport::tracing::debug!(?ev, "EventRegistry sending event");
                self.tx.try_send(ev).map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TrySendError::Full(_) => unreachable!("unbounded channel cannot be full"),
                    ::oyui_tasker::reexport::async_channel::TrySendError::Closed(val) => ::tokio::sync::mpsc::error::SendError(val),
                })
            }

            pub async fn recv(&self) -> Option<Event> {
                self.rx.recv().await.ok()
            }

            pub fn try_recv(&self) -> Result<Event, ::tokio::sync::mpsc::error::TryRecvError> {
                self.rx.try_recv().map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TryRecvError::Empty => ::tokio::sync::mpsc::error::TryRecvError::Empty,
                    ::oyui_tasker::reexport::async_channel::TryRecvError::Closed => ::tokio::sync::mpsc::error::TryRecvError::Disconnected,
                })
            }

            pub fn into_split(self) -> (EventSender, EventReceiver, Option<::tokio::task::JoinHandle<()>>) {
                (
                    EventSender { tx: self.tx },
                    EventReceiver { rx: self.rx },
                    self.handle.into_inner().unwrap()
                )
            }

            pub async fn shutdown(&self) -> Result<(), ::tokio::sync::mpsc::error::SendError<Event>> {
                self.tx.try_send(Event::Shutdown).map_err(|e| match e {
                    ::oyui_tasker::reexport::async_channel::TrySendError::Full(_) => unreachable!("unbounded channel cannot be full"),
                    ::oyui_tasker::reexport::async_channel::TrySendError::Closed(val) => ::tokio::sync::mpsc::error::SendError(val),
                })?;
                let handle = self.handle.lock().unwrap().take();
                if let Some(handle) = handle {
                    let _ = handle.await;
                }
                Ok(())
            }

            pub fn spawn<C>(ctx: C) -> Self
            where
                C: Send + Sync + Clone + 'static,
                #( #spawn_bounds, )*
            {
                let c = ::std::sync::Arc::new(ctx);
                let (req_tx, req_rx) = ::oyui_tasker::reexport::async_channel::unbounded::<Event>();
                let (ev_tx, ev_rx) = ::oyui_tasker::reexport::async_channel::unbounded::<Event>();

                let tx_clone = EventSender { tx: req_tx.clone() };

                let handle = ::tokio::spawn(async move {
                    use ::oyui_tasker::reexport::tracing::Instrument;

                    async move {
                        while let Ok(event) = req_rx.recv().await {
                            ::oyui_tasker::reexport::tracing::debug!(?event, "Processing event");
                            match event {
                                Event::Shutdown => {
                                    let _ = ev_tx.try_send(Event::Shutdown);
                                    break;
                                }
                                #( #match_arms )*
                            }
                        }
                    }
                    .instrument(::oyui_tasker::reexport::tracing::info_span!("event_registry_worker_loop"))
                    .await;
                });

                Self {
                    tx: req_tx,
                    rx: ev_rx,
                    handle: ::std::sync::Mutex::new(Some(handle)),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
