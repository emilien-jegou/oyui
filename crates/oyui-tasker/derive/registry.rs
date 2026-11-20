//! Codegen for `tasker_registry!`: event enum, sender/receiver, and dispatch loop.

use proc_macro::TokenStream;
use quote::quote;

use crate::input::RegistryInput;
use syn::Type;

/// The payload inside `Asked<X>`, or the type unchanged for anything else.
///
/// A request event is declared with an `Asked<X>` payload because that is what
/// the channel carries, but callers and listeners think in terms of `X`.
fn asked_inner(ty: &Type) -> Type {
    let syn::Type::Path(path) = ty else {
        return ty.clone();
    };
    let Some(last) = path.path.segments.last() else {
        return ty.clone();
    };
    if last.ident != "Asked" {
        return ty.clone();
    }
    match &last.arguments {
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
            match args.args.first() {
                Some(syn::GenericArgument::Type(inner)) => inner.clone(),
                _ => ty.clone(),
            }
        }
        _ => ty.clone(),
    }
}

/// Generates the registry module contents for a parsed `tasker_registry!` input.
pub(crate) fn expand(input: &RegistryInput) -> TokenStream {
    // `(variant, channel payload, payload listeners see, listeners)`.
    //
    // The channel payload differs from the listener payload only for requests
    // declared in `replies`: the channel carries `Asked<T>` so the reply can be
    // routed, and the loop unwraps it before handing over the `T` the caller
    // sent, so no listener ever has to thread a token through anything.
    let mut mapped_events = Vec::new();
    for event in &input.events {
        let event_name = &event.name;
        let channel_ty = &event.ty;
        let is_reply = input.replies.iter().any(|r| r.event_name == *event_name);
        let listener_ty = if is_reply {
            asked_inner(channel_ty)
        } else {
            channel_ty.clone()
        };

        let event_listeners = input
            .listeners
            .iter()
            .find(|l| l.event_name == *event_name)
            .map(|l| &l.listeners)
            .cloned()
            .unwrap_or_default();

        mapped_events.push((event_name, channel_ty.clone(), listener_ty, event_listeners));
    }

    let enum_variants = mapped_events.iter().map(|(name, ty, _, _)| {
        quote! { #name(#ty) }
    });

    let from_impls = mapped_events.iter().map(|(name, ty, _, _)| {
        quote! {
            impl From<#ty> for Event {
                fn from(ev: #ty) -> Self {
                    Event::#name(ev)
                }
            }
        }
    });

    let spawn_bounds = mapped_events.iter().flat_map(|(_, _, ty, listeners)| {
        listeners.iter().map(move |listener_ty| {
            quote! {
                <#listener_ty as ::oyui_tasker::worker::Listener<#ty>>::Context: ::oyui_tasker::worker::ExtractsFrom<C>
            }
        })
    });

    let match_arms = mapped_events.iter().map(|(name, _channel_ty, effective_ty, listeners)| {
        // A request declared in `replies` carries the caller's token: its
        // listeners must be handed it, and it is never mirrored (the answer is
        // routed privately, so there is nothing to broadcast).
        let is_reply = input.replies.iter().any(|other| other.event_name == **name);

        let (token_binding, payload_binding) = if is_reply {
            (
                quote! { let token = ::std::option::Option::Some(ev.token); },
                quote! { let ev = ev.inner; },
            )
        } else {
            (
                quote! { let token: ::std::option::Option<::oyui_tasker::worker::ReplyToken> = ::std::option::Option::None; },
                quote! {},
            )
        };

        let listener_spawns = listeners.iter().map(|listener_ty| {
            quote! {
                // Own handle per listener: an event can fan out to several, and
                // each needs its own sender to report a failure through. Named
                // distinctly so a second spawn still sees the outer binding.
                let err_tx = ev_tx.clone();
                let ctx_extracted = <<#listener_ty as ::oyui_tasker::worker::Listener<#effective_ty>>::Context as ::oyui_tasker::worker::ExtractsFrom<C>>::extract(&*c);
                let ev_clone = ev.clone();
                let tx = EventSender {
                    tx: req_tx.clone(),
                    token,
                    replies: replies.clone(),
                };

                ::tokio::spawn(
                    async move {
                        let span = ::oyui_tasker::reexport::tracing::info_span!(
                            "listener_handle",
                            event_type = stringify!(#name),
                            listener = stringify!(#listener_ty)
                        );

                        use ::oyui_tasker::reexport::tracing::Instrument;
                        async {
                            let res = <#listener_ty as ::oyui_tasker::worker::Listener<#effective_ty>>::handle(ev_clone, ctx_extracted, tx).await;
                            match res {
                                Ok(_) => {
                                    ::oyui_tasker::reexport::tracing::trace!("Listener completed successfully");
                                }
                                Err(e) => {
                                    // A listener that fails has no other way to
                                    // reach the consumer: it is not awaited by
                                    // anyone, so its `Err` would otherwise only
                                    // ever reach the log.
                                    ::oyui_tasker::reexport::tracing::error!(error = ?e, "Listener failed");
                                    let _ = err_tx.try_send(Event::ListenerFailed(::oyui_tasker::ListenerFailed {
                                        event: stringify!(#name),
                                        listener: stringify!(#listener_ty),
                                        error: e.to_string(),
                                    }));
                                }
                            }
                        }
                        .instrument(span)
                        .await
                    }
                );
            }
        });

        // Routing class of this variant: never mirrored, mirrored with burst
        // coalescing, or mirrored as-is (payload the app must not lose).
        let is_internal = input.internal.iter().any(|other| other == *name);
        let is_collapse = input.collapse.iter().any(|other| other == *name);
        let mirror_stmt = if is_internal || is_reply {
            quote! {}
        } else if is_collapse {
            quote! {
                coalesce_send(&ev_rx, &ev_tx, Event::#name(ev.clone()));
            }
        } else {
            quote! {
                let _ = ev_tx.try_send(Event::#name(ev.clone()));
            }
        };

        quote! {
            Event::#name(ev) => {
                #token_binding
                #payload_binding
                #mirror_stmt
                #( #listener_spawns )*
            }
        }
    });

    let internal_idents = &input.internal;
    let collapse_idents = &input.collapse;

    // `(RequestPayload, ResponseType)` per declared pairing.
    //
    // The request type is the *payload* the caller holds (`Question`, not
    // `Asked<Question>`), so `ask` binds `R: ReplyOf<E>` on what the caller
    // actually passes rather than on the wrapper the channel carries.
    let reply_pairs: Vec<(Type, Type)> = input
        .replies
        .iter()
        .map(|reply| {
            let declared = input
                .events
                .iter()
                .find(|e| e.name == reply.event_name)
                .map(|e| e.ty.clone())
                .unwrap_or_else(|| reply.response.clone());
            (asked_inner(&declared), reply.response.clone())
        })
        .collect();
    let reply_req = reply_pairs.iter().map(|(req, _)| req);
    let reply_res = reply_pairs.iter().map(|(_, res)| res);

    let debug_arms = mapped_events.iter().map(|(name, _, _, _)| {
        quote! { Event::#name(_) => stringify!(#name), }
    });

    let expanded = quote! {
        /// The registry event union plus a `Shutdown` control variant.
        #[derive(Clone)]
        pub enum Event {
            #( #enum_variants, )*
            /// A listener returned an error; generated by the dispatch loop.
            ListenerFailed(::oyui_tasker::ListenerFailed),
            Shutdown,
        }

        impl ::std::fmt::Debug for Event {
            /// Prints the variant name; payloads are intentionally omitted.
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(match self {
                    #( #debug_arms )*
                    Event::ListenerFailed(_) => "ListenerFailed",
                    Event::Shutdown => "Shutdown",
                })
            }
        }

        impl Event {
            /// True when the event is internal plumbing that never reaches the
            /// app-facing receiver.
            ///
            /// Declared with `internal = [...]`. Worker-to-worker events still
            /// dispatch to their listeners; they simply stop waking the app,
            /// which is where an uncontrolled repaint per event used to come
            /// from.
            pub fn is_internal(&self) -> bool {
                match self {
                    #( Event::#internal_idents(_) => true, )*
                    _ => false,
                }
            }

            /// True when bursts of this event are coalesced before the app sees
            /// them, keeping only the newest pending copy.
            ///
            /// Declared with `collapse = [...]`. Only sound for events the app
            /// reacts to by re-reading shared state rather than by consuming a
            /// sequence of payloads.
            pub fn is_collapsing(&self) -> bool {
                match self {
                    #( Event::#collapse_idents(_) => true, )*
                    _ => false,
                }
            }
        }

        /// Queues `ev` for the app, first dropping a queued copy of the same
        /// variant.
        ///
        /// A burst of per-file results only needs the app to look once at the
        /// current shared state, so the newest of the burst wins and stale
        /// siblings are dropped instead of queueing behind it. Other variants
        /// keep their relative order.
        fn coalesce_send(
            rx: &::oyui_tasker::reexport::async_channel::Receiver<Event>,
            tx: &::oyui_tasker::reexport::async_channel::Sender<Event>,
            ev: Event,
        ) {
            let stale_variant = ::std::mem::discriminant(&ev);
            let mut pending = ::std::vec::Vec::new();
            while let Ok(queued) = rx.try_recv() {
                if ::std::mem::discriminant(&queued) != stale_variant {
                    pending.push(queued);
                }
            }
            pending.push(ev);

            for queued in pending {
                if tx.try_send(queued).is_err() {
                    break;
                }
            }
        }

        #( #from_impls )*

        // Declared request/response pairings, enforced at the call site.
        #(
            impl ::oyui_tasker::worker::ReplyOf<#reply_req> for #reply_res {}
        )*

        fn tasker_try_send(
            tx: &::oyui_tasker::reexport::async_channel::Sender<Event>,
            ev: Event,
        ) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>> {
            ::oyui_tasker::reexport::tracing::debug!(?ev, "sending event");
            tx.try_send(ev)
        }

        /// Clonable handle that queues events into the registry loop.
        ///
        /// A sender handed to a listener for an [`Asked`] dispatch also carries
        /// the reply token, so `reply` needs no id threaded through the payload.
        #[derive(Clone)]
        pub struct EventSender {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
            /// Token of the caller this dispatch answers, when there is one.
            token: ::std::option::Option<::oyui_tasker::worker::ReplyToken>,
            /// Reply slots shared with the registry.
            replies: ::oyui_tasker::worker::Replies,
        }

        impl EventSender {
            /// Queues `event` for dispatch; fails only once the registry is down.
            pub fn send<E>(&self, event: E) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>>
            where
                Event: From<E>,
            {
                tasker_try_send(&self.tx, Event::from(event))
            }

            /// Answers the request this sender is scoped to.
            ///
            /// Fails when the dispatch was not an `Asked` one, when the answer
            /// is not the type declared for the request, or when the caller has
            /// already gone away. In every case the listener keeps running, so a
            /// lost answer costs a report and not the listener.
            pub fn reply<R>(&self, res: R) -> eyre::Result<()>
            where
                R: Send + 'static,
            {
                let token = self.token.ok_or_else(|| {
                    eyre::eyre!("reply() called outside an `Asked` dispatch; \
                                the request was answered by nobody")
                })?;
                self.replies.deliver(token, res)
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

            /// Number of mirrored events waiting to be consumed.
            ///
            /// Coalescing keeps this near zero even under heavy load, which
            /// makes it a cheap guard against a loop that queues frames faster
            /// than it paints them.
            pub fn len(&self) -> usize {
                self.rx.len()
            }

            /// True when no mirrored event is waiting.
            pub fn is_empty(&self) -> bool {
                self.rx.is_empty()
            }
        }

        /// Owns the registry channels and the dispatch loop task.
        pub struct EventRegistry {
            tx: ::oyui_tasker::reexport::async_channel::Sender<Event>,
            rx: ::oyui_tasker::reexport::async_channel::Receiver<Event>,
            /// Reply slots parked by `ask`.
            replies: ::oyui_tasker::worker::Replies,
            /// Source of reply tokens; monotonic so ids are never reused.
            next_reply: ::std::sync::atomic::AtomicU64,
            handle: ::std::sync::Mutex<Option<::tokio::task::JoinHandle<()>>>,
        }

        impl EventRegistry {
            /// Clones a send handle to the registry loop.
            pub fn sender(&self) -> EventSender {
                self.scoped_sender(::std::option::Option::None)
            }

            /// Builds a sender, optionally scoped to the reply it must answer.
            fn scoped_sender(
                &self,
                token: ::std::option::Option<::oyui_tasker::worker::ReplyToken>,
            ) -> EventSender {
                EventSender {
                    tx: self.tx.clone(),
                    token,
                    replies: self.replies.clone(),
                }
            }

            /// Queues `event` for dispatch; fails only once the registry is down.
            pub fn send<E>(&self, event: E) -> Result<(), ::oyui_tasker::reexport::async_channel::TrySendError<Event>>
            where
                Event: From<E>,
            {
                tasker_try_send(&self.tx, Event::from(event))
            }

            /// Sends `event` and returns the handle to its answer.
            ///
            /// The caller keeps its own continuation: the answer arrives at this
            /// `Reply` rather than on the shared event stream, so nobody has to
            /// match ids afterwards, and the reply slot is released when the
            /// handle is dropped. A request nobody asked for cannot be built:
            /// the pairing has to be declared in `replies = [...]`.
            pub fn ask<E, R>(&self, event: E) -> eyre::Result<::oyui_tasker::worker::Reply<R>>
            where
                E: Send + 'static,
                R: ::oyui_tasker::worker::ReplyOf<E> + Send + 'static,
                Event: From<::oyui_tasker::worker::Asked<E>>,
            {
                let token = ::oyui_tasker::worker::ReplyToken(
                    self.next_reply
                        .fetch_add(1, ::std::sync::atomic::Ordering::Relaxed),
                );
                let (tx, rx) = ::tokio::sync::oneshot::channel();
                self.replies.insert(token, tx);

                let asked = ::oyui_tasker::worker::Asked::new(event, token);
                // `send` converts through the same bound, so no cast here.
                if let Err(e) = self.send(asked) {
                    // Nobody will ever answer it; do not leave the slot behind.
                    self.replies.forget(token);
                    return Err(eyre::eyre!("queuing the request failed: {e}"));
                }

                Ok(::oyui_tasker::worker::Reply::new(
                    rx,
                    self.replies.clone(),
                    token,
                ))
            }

            /// Number of requests still awaiting an answer.
            pub fn pending_replies(&self) -> usize {
                self.replies.len()
            }

            /// Awaits the next mirrored event; `None` once the registry is down.
            pub async fn recv(&self) -> Option<Event> {
                self.rx.recv().await.ok()
            }

            /// Polls the next mirrored event without waiting.
            pub fn try_recv(&self) -> Result<Event, ::oyui_tasker::reexport::async_channel::TryRecvError> {
                self.rx.try_recv()
            }

            /// Number of mirrored events waiting to be consumed.
            pub fn pending(&self) -> usize {
                self.rx.len()
            }

            /// Splits off a sender, a receiver, and the dispatch task handle.
            pub fn into_split(self) -> (EventSender, EventReceiver, Option<::tokio::task::JoinHandle<()>>) {
                let EventRegistry {
                    tx,
                    rx,
                    replies,
                    next_reply,
                    handle,
                } = self;
                let sender = EventSender {
                    tx,
                    token: ::std::option::Option::None,
                    replies,
                };
                let _ = next_reply;
                (sender, EventReceiver { rx }, handle.into_inner().unwrap())
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
                let replies = ::oyui_tasker::worker::Replies::default();

                let tx_clone = EventSender {
                    tx: req_tx.clone(),
                    token: ::std::option::Option::None,
                    replies: replies.clone(),
                };
                // Owned by the loop: coalescing must drain what the app has not
                // consumed yet, which only the loop is in a position to do.
                let loop_ev_rx = ev_rx.clone();
                // Reply routing is shared with the senders the loop hands out,
                // so a listener can answer whoever issued the request.
                let loop_replies = replies.clone();
                let loop_req_tx = req_tx.clone();

                let handle = ::tokio::spawn(async move {
                    // Local names the generated dispatch arms mirror through.
                    let ev_rx = loop_ev_rx;
                    let replies = loop_replies;
                    let req_tx = loop_req_tx;
                    use ::oyui_tasker::reexport::tracing::Instrument;

                    async move {
                        while let Ok(event) = req_rx.recv().await {
                            ::oyui_tasker::reexport::tracing::debug!(?event, "Processing event");
                            match event {
                                Event::Shutdown => {
                                    let _ = ev_tx.try_send(Event::Shutdown);
                                    break;
                                }
                                // Emitted by the loop itself, straight to the
                                // mirror channel: never routed back in.
                                Event::ListenerFailed(_) => {}
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
                    replies,
                    next_reply: ::std::sync::atomic::AtomicU64::new(0),
                    handle: ::std::sync::Mutex::new(Some(handle)),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
