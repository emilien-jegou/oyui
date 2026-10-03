//! Listener and context-extraction traits consumed by the generated registry loop.

/// A trait used for Dependency Injection into listener contexts.
pub trait ExtractsFrom<C> {
    /// Builds the context value from the registry-wide context.
    fn extract(ctx: &C) -> Self;
}

impl<C> ExtractsFrom<C> for () {
    fn extract(_ctx: &C) -> Self {}
}

/// A listener reacting to event `E`, emitting follow-ups through `Sender`.
pub trait Listener<E>: Send + Sync + 'static {
    /// Sender handed to `handle`; must be the registry's event sender.
    type Sender: Send + Sync + 'static;
    /// Per-listener context slice extracted from the registry context.
    type Context: Send + Sync + 'static;

    /// Handles one event; the registry loop logs any returned error.
    fn handle(
        event: E,
        ctx: Self::Context,
        tx: Self::Sender,
    ) -> impl std::future::Future<Output = eyre::Result<()>> + Send;
}
