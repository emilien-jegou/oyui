//! Codegen for `tasker_registry!`: event enum, sender/receiver, and dispatch loop.

use proc_macro::TokenStream;
use quote::quote;

use crate::input::RegistryInput;

/// Generates the registry module contents for a parsed `tasker_registry!` input.
pub(crate) fn expand(input: &RegistryInput) -> TokenStream {
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
                <#listener_ty as ::oyui_tasker::worker::Listener<#ty>>::Context: ::oyui_tasker::worker::ExtractsFrom<C>
            }
        })
    });

    let match_arms = mapped_events.iter().map(|(name, ty, listeners)| {
        let listener_spawns = listeners.iter().map(|listener_ty| {
            quote! {
                let ctx_extracted = <<#listener_ty as ::oyui_tasker::worker::Listener<#ty>>::Context as ::oyui_tasker::worker::ExtractsFrom<C>>::extract(&*c);
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
                            let res = <#listener_ty as ::oyui_tasker::worker::Listener<#ty>>::handle(ev_clone, ctx_extracted, tx).await;
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

    let debug_arms = mapped_events.iter().map(|(name, _, _)| {
        quote! { Event::#name(_) => stringify!(#name), }
    });

    let expanded = quote! {
        /// The registry event union plus a `Shutdown` control variant.
        #[derive(Clone)]
        pub enum Event {
            #( #enum_variants, )*
            Shutdown,
        }

        impl ::std::fmt::Debug for Event {
            /// Prints the variant name; payloads are intentionally omitted.
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(match self {
                    #( #debug_arms )*
                    Event::Shutdown => "Shutdown",
                })
            }
        }

        #( #from_impls )*

        fn tasker_try_send(
            tx: &::oyui_tasker::reexport::async_channel::Sender<Event>,
            ev: Event,
        ) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>> {
            ::oyui_tasker::reexport::tracing::debug!(?ev, "sending event");
            tx.try_send(ev)
        }

        /// Clonable handle that queues events into the registry loop.
        #[derive(Clone)]
        pub struct EventSender {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
        }

        impl EventSender {
            /// Queues `event` for dispatch; fails only once the registry is down.
            pub fn send<E>(&self, event: E) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>>
            where
                Event: From<E>,
            {
                tasker_try_send(&self.tx, Event::from(event))
            }

            /// Queues the `Shutdown` control event.
            pub fn shutdown(&self) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>> {
                tasker_try_send(&self.tx, Event::Shutdown)
            }
        }

        /// Receiving end of the registry's mirrored event stream.
        pub struct EventReceiver {
            rx: ::oyui_tasker::reexport::async_channel::Receiver<Event>,
        }

        impl EventReceiver {
            /// Awaits the next mirrored event; `None` once the registry is down.
            pub async fn recv(&self) -> Option<Event> {
                self.rx.recv().await.ok()
            }

            /// Polls the next mirrored event without waiting.
            pub fn try_recv(&self) -> Result<Event, ::oyui_tasker::reexport::async_channel::TryRecvError> {
                self.rx.try_recv()
            }
        }

        /// Owns the registry channels and the dispatch loop task.
        pub struct EventRegistry {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
            rx: ::oyui_tasker::reexport::async_channel::Receiver<Event>,
            handle: ::std::sync::Mutex<Option<::tokio::task::JoinHandle<()>>>,
        }

        impl EventRegistry {
            /// Clones a send handle to the registry loop.
            pub fn sender(&self) -> EventSender {
                EventSender { tx: self.tx.clone() }
            }

            /// Queues `event` for dispatch; fails only once the registry is down.
            pub fn send<E>(&self, event: E) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>>
            where
                Event: From<E>,
            {
                tasker_try_send(&self.tx, Event::from(event))
            }

            /// Awaits the next mirrored event; `None` once the registry is down.
            pub async fn recv(&self) -> Option<Event> {
                self.rx.recv().await.ok()
            }

            /// Polls the next mirrored event without waiting.
            pub fn try_recv(&self) -> Result<Event, ::oyui_tasker::reexport::async_channel::TryRecvError> {
                self.rx.try_recv()
            }

            /// Splits off a sender, a receiver, and the dispatch task handle.
            pub fn into_split(self) -> (EventSender, EventReceiver, Option<::tokio::task::JoinHandle<()>>) {
                (
                    EventSender { tx: self.tx },
                    EventReceiver { rx: self.rx },
                    self.handle.into_inner().unwrap()
                )
            }

            /// Stops the dispatch loop and waits for it to exit.
            pub async fn shutdown(&self) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>> {
                tasker_try_send(&self.tx, Event::Shutdown)?;
                let handle = self.handle.lock().unwrap().take();
                if let Some(handle) = handle {
                    let _ = handle.await;
                }
                Ok(())
            }

            /// Spawns the dispatch loop with `ctx` extracted per listener.
            pub fn spawn<C>(ctx: C) -> Self
            where
                C: Send + Sync + 'static,
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
