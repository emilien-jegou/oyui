//! Listener and context-extraction traits consumed by the generated registry loop.
//!
//! The registry is a broadcast bus: every event is cloned for the mirror and
//! again for each listener. That is the right shape for state a listener
//! publishes, and the wrong one for an answer one caller asked for, which is
//! why [`Asked`] and [`Reply`] exist alongside it.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

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
    /// Sender handed to `handle`; must be the registry's generated `EventSender`.
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

/// A listener returned an error.
///
/// Listeners run as detached tasks nobody awaits, so an `Err` has no caller to
/// propagate to. The registry mirrors this to the consumer instead, which turns
/// "a background computation silently did not happen" into something a UI can
/// show — the difference between a stale view with no explanation and a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenerFailed {
    /// Name of the event whose listener failed.
    pub event: &'static str,
    /// Name of the failing listener.
    pub listener: &'static str,
    /// Rendered error.
    pub error: String,
}

impl std::fmt::Display for ListenerFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} failed: {}", self.listener, self.error)
    }
}

/// Correlation handle for one in-flight request.
///
/// Opaque: it only exists to route an answer back to whoever asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReplyToken(pub u64);

impl ReplyToken {
    /// The numeric id, for logs and diagnostics.
    pub fn id(self) -> u64 {
        self.0
    }
}

/// Marks `Res` as the answer to `Req`.
///
/// Generated from the registry's `replies = [...]` declaration, so the
/// request/response pairing is checked at compile time instead of being
/// guessed from a field the payload happens to carry.
pub trait ReplyOf<Req>: 'static {}

/// A request tagged with the token of the reply its issuer is waiting on.
///
/// Produced by [`EventRegistry::ask`](crate::tasker_registry)-generated code;
/// never sent through plain `send`, because a request sent that way has nobody
/// to answer.
#[derive(Clone, Copy, Debug)]
pub struct Asked<E> {
    /// The request payload.
    pub inner: E,
    /// Token identifying the caller waiting for the answer.
    pub token: ReplyToken,
}

impl<E> Asked<E> {
    /// Pairs a payload with its reply token.
    pub fn new(inner: E, token: ReplyToken) -> Self {
        Self { inner, token }
    }
}

/// Live reply slots, keyed by token.
///
/// Shared by the registry (which parks a slot per `ask`) and by the senders it
/// hands to listeners (which deliver into one).
#[derive(Clone, Default)]
pub struct Replies(Arc<Mutex<HashMap<u64, Box<dyn Any + Send>>>>);

impl Replies {
    /// Parks the caller's reply slot.
    ///
    /// The sender is held in an `Option` so a delivered reply can be taken out
    /// of the slot, which makes a second delivery fail rather than succeed
    /// against a channel that is already closed.
    pub fn insert<R: Send + 'static>(&self, token: ReplyToken, tx: oneshot::Sender<R>) {
        self.0.lock().unwrap().insert(token.0, Box::new(Some(tx)));
    }

    /// Hands `res` to whoever asked.
    ///
    /// A wrong answer type is refused and the slot is left in place, so a
    /// mis-typed reply cannot strand a caller.
    pub fn deliver<R: Send + 'static>(&self, token: ReplyToken, res: R) -> eyre::Result<()> {
        let mut slots = self.0.lock().unwrap();
        let slot = slots
            .get_mut(&token.0)
            .ok_or_else(|| eyre::eyre!("no reply slot for token {}", token.0))?;
        let pending = slot
            .downcast_mut::<Option<oneshot::Sender<R>>>()
            .ok_or_else(|| eyre::eyre!("reply type mismatch for token {}", token.0))?;
        let tx = pending
            .take()
            .ok_or_else(|| eyre::eyre!("token {} was already answered", token.0))?;
        tx.send(res)
            .map_err(|_| eyre::eyre!("reply receiver dropped for token {}", token.0))
    }

    /// Drops the slot.
    ///
    /// Called when a caller loses interest, so a later reply is reported as
    /// undeliverable instead of sitting in the map forever.
    pub fn forget(&self, token: ReplyToken) {
        self.0.lock().unwrap().remove(&token.0);
    }

    /// Number of parked slots, i.e. requests still awaiting an answer.
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// True when no request is awaiting an answer.
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
}

/// The caller's end of one request.
///
/// Dropping it releases the reply slot, so a lost request cannot leave state
/// behind in the registry — the property that makes scoped correlation cheaper
/// to reason about than a hand-rolled id table.
pub struct Reply<R> {
    /// Taken out by `recv`, which needs to own the channel to await it.
    rx: Option<oneshot::Receiver<R>>,
    replies: Replies,
    token: ReplyToken,
}

impl<R> Reply<R> {
    pub fn new(rx: oneshot::Receiver<R>, replies: Replies, token: ReplyToken) -> Self {
        Self {
            rx: Some(rx),
            replies,
            token,
        }
    }

    /// The token this reply is waiting on.
    pub fn token(&self) -> ReplyToken {
        self.token
    }

    /// Awaits the answer, releasing the slot either way.
    pub async fn recv(&mut self) -> Option<R> {
        let rx = self.rx.take()?;
        let answer = rx.await.ok();
        self.replies.forget(self.token);
        answer
    }

    /// Polls for the answer without awaiting.
    ///
    /// Lets a single-threaded consumer resolve replies alongside its other
    /// work, which is what a UI loop needs when the answer must be delivered on
    /// the same thread that issued the request.
    pub fn try_recv(&mut self) -> Option<R> {
        self.rx.as_mut()?.try_recv().ok()
    }
}

impl<R> Drop for Reply<R> {
    fn drop(&mut self) {
        self.replies.forget(self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Req;
    struct Res(u8);

    impl ReplyOf<Req> for Res {}

    fn parked<R: Send + 'static>() -> (Reply<R>, Replies, ReplyToken) {
        let replies = Replies::default();
        let (tx, rx) = oneshot::channel();
        let token = ReplyToken(7);
        replies.insert(token, tx);
        (Reply::new(rx, replies.clone(), token), replies, token)
    }

    #[test]
    fn deliver_routes_to_the_waiting_caller() {
        let (mut reply, replies, token) = parked();
        assert_eq!(replies.len(), 1);

        replies.deliver(token, Res(3)).expect("delivered");
        assert_eq!(reply.try_recv().map(|r: Res| r.0), Some(3));
    }

    /// A reply arriving for a caller that already left must not panic, and must
    /// not be able to deliver into the wrong slot.
    #[test]
    fn deliver_to_an_unknown_token_is_reported() {
        let (_reply, replies, token) = parked::<Res>();
        replies.forget(token);
        assert!(replies.is_empty());

        let err = replies
            .deliver(token, Res(1))
            .expect_err("no slot to deliver into");
        assert!(err.to_string().contains("no reply slot"), "got: {err}");
    }

    /// Dropping the caller must release the slot: otherwise the registry grows
    /// by one entry per abandoned request.
    #[test]
    fn dropping_the_caller_releases_the_slot() {
        let (reply, replies, token) = parked::<Res>();

        drop(reply);

        assert!(replies.is_empty(), "slot must be freed on drop");
        let err = replies
            .deliver(token, Res(9))
            .expect_err("dropped caller leaves nothing to deliver to");
        assert!(err.to_string().contains("no reply slot"), "got: {err}");
    }

    /// A mis-typed answer is refused and the slot survives, so a later correct
    /// one can still land.
    #[test]
    fn a_mis_typed_reply_leaves_the_slot_intact() {
        let (mut reply, replies, token) = parked::<Res>();

        let err = replies
            .deliver(token, 42u8)
            .expect_err("wrong type must be refused");
        assert!(err.to_string().contains("type mismatch"), "got: {err}");
        assert_eq!(replies.len(), 1, "slot must survive for the real answer");

        replies
            .deliver(token, Res(5))
            .expect("correct answer lands");
        assert_eq!(reply.try_recv().map(|r: Res| r.0), Some(5));
    }
}
