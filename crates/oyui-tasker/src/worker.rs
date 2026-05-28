/// A trait used for Dependency Injection into listener contexts.
pub trait ExtractsFrom<C> {
    fn extract(ctx: &C) -> Self;
}

impl<C> ExtractsFrom<C> for () {
    fn extract(_ctx: &C) -> Self {}
}

/// A listener trait configured with a specific Event type (`E`) and an EventSender (`S`).
pub trait Listener<E, S>: Send + Sync + 'static {
    type Context: Send + Sync + 'static;

    fn handle(
        event: E,
        ctx: Self::Context,
        tx: S,
    ) -> impl std::future::Future<Output = eyre::Result<()>> + Send;
}
