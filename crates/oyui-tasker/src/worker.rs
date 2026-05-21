/// A trait used for Dependency Injection into worker tasks.
/// 
/// Types that implement this know how to extract themselves from a given Context `C`.
pub trait ExtractsFrom<C> {
    fn extract(ctx: &C) -> Self;
}

// Automatically provide `()` for tasks that do not require any context.
impl<C> ExtractsFrom<C> for () {
    fn extract(_ctx: &C) -> Self {}
}

pub trait WorkerTask {
    type Request: Send + Clone + std::fmt::Debug + 'static;
    type Response: Send + std::fmt::Debug + 'static;
    type Context: Send + 'static;

    fn handle(
        req: Self::Request,
        ctx: Self::Context,
    ) -> impl std::future::Future<Output = Self::Response> + Send;
}
