use oyui_tasker::worker::Asked;
use oyui_tasker::worker::Reply as ReplyHandle;
use oyui_tasker::{tasker_registry, Listener, TaskerProvide};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Echo {
    pub msg: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchoResult {
    pub msg: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Math {
    pub values: (i32, i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathResult {
    pub value: i32,
}

/// Plumbing between listeners: the app has no use for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pong {
    pub count: u32,
}

/// Bursty recomputation result: the app only repaints once per burst.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burst {
    pub n: u32,
}

/// A question, answered only for whoever asked it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub id: u32,
    pub text: String,
}

pub struct EchoListener;
impl Listener<Echo> for EchoListener {
    type Sender = EventSender;
    type Context = ();

    async fn handle(event: Echo, _ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tx.send(EchoResult {
            msg: format!("echo: {}", event.msg),
        })?;
        Ok(())
    }
}

pub struct MathListener;

impl Listener<Math> for MathListener {
    type Sender = EventSender;
    type Context = i32;

    async fn handle(event: Math, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tx.send(MathResult {
            value: (event.values.0 + event.values.1) * ctx,
        })?;
        Ok(())
    }
}

/// Always fails, to prove a detached listener's error still surfaces.
pub struct ExplodingListener;

impl Listener<Math> for ExplodingListener {
    type Sender = EventSender;
    type Context = ();

    async fn handle(_: Math, _: Self::Context, _: EventSender) -> eyre::Result<()> {
        eyre::bail!("intentional listener failure");
    }
}

/// Internal plumbing: still dispatched, just invisible to the app.
pub struct PlumbingListener;

impl Listener<Pong> for PlumbingListener {
    type Sender = EventSender;
    type Context = ();

    async fn handle(event: Pong, _ctx: Self::Context, _tx: EventSender) -> eyre::Result<()> {
        // Re-emitting an app-visible event from an internal listener proves the
        // internal route still ran.
        _tx.send(Burst { n: event.count })?;
        Ok(())
    }
}

/// Answers the question it was handed.
///
/// It routes through the sender rather than an id echoed in the payload: the
/// listener never learns which caller asked, only the answer and where it goes.
pub struct AnsweringListener;

impl Listener<Question> for AnsweringListener {
    type Sender = EventSender;
    type Context = i32;

    async fn handle(event: Question, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tx.reply(Answer {
            id: event.id,
            text: format!("by multiplier {}", ctx),
        })?;
        Ok(())
    }
}

#[derive(Clone, TaskerProvide)]
pub struct AppContext {
    pub multiplier: i32,
}

tasker_registry! {
    events = [
        Echo       => Echo,
        EchoResult => EchoResult,
        Math       => Math,
        MathResult => MathResult,
        Pong       => Pong,
        Burst      => Burst,
        AskedQ     => Asked<Question>,
        Answer     => Answer,
    ],
    replies = [
        AskedQ => Answer,
    ],
    internal = [Pong, AskedQ],
    collapse = [Burst],
    listeners = [
        Echo    => [EchoListener],
        Math    => [MathListener, ExplodingListener],
        Pong    => [PlumbingListener],
        AskedQ  => [AnsweringListener],
    ],
}

/// Drains every mirrored event until the queue stays empty for a moment.
///
/// Generic over the registry and the receiver handed out by `into_split`, so
/// tests need not assume how many events a fan-out produces.
async fn drain<T: Mirror>(registry: &T) -> Vec<Event> {
    let mut seen = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(std::time::Duration::from_millis(20), registry.recv()).await
    {
        seen.push(event);
    }
    seen
}

/// Anything that can await a mirrored event.
pub trait Mirror {
    fn recv(&self) -> impl std::future::Future<Output = Option<Event>> + Send;
}

impl Mirror for EventRegistry {
    async fn recv(&self) -> Option<Event> {
        self.recv().await
    }
}

impl Mirror for EventReceiver {
    async fn recv(&self) -> Option<Event> {
        self.recv().await
    }
}

/// An event declared `internal` still reaches its listeners but never the
/// app-facing receiver, so it cannot wake the app into a repaint.
#[tokio::test]
async fn internal_events_dispatch_but_are_not_mirrored() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 2 });

    // Pong is internal: its listener still runs, proven by the `Burst` it
    // re-emits, but Pong itself never reaches the app.
    registry.send(Pong { count: 7 }).unwrap();

    let events = drain(&registry).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Burst(b) if b.n == 7)),
        "an internal event must still dispatch to listeners: {events:?}"
    );
    assert!(
        !events.iter().any(|e| e.is_internal()),
        "an internal event must never reach the app receiver: {events:?}"
    );

    assert!(Event::Pong(Pong { count: 1 }).is_internal());
    assert!(!Event::MathResult(MathResult { value: 0 }).is_internal());

    registry.shutdown().await.unwrap();
}

/// A burst of `collapse` events leaves a single pending copy, so the app pays
/// one repaint instead of one per file.
#[tokio::test]
async fn collapsing_events_are_coalesced() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 1 });

    for n in 0..64 {
        registry.send(Burst { n }).unwrap();
    }

    let first = registry.recv().await.expect("burst still signals the app");
    assert!(
        matches!(first, Event::Burst(ref b) if b.n == 63),
        "only the newest of a burst survives: {first:?}"
    );
    assert!(
        registry.try_recv().is_err(),
        "the 63 stale siblings must have been dropped, not queued"
    );

    // A collapse variant is still mirrored (unlike an internal one).
    assert!(Event::Burst(Burst { n: 0 }).is_collapsing());
    assert!(!Event::Burst(Burst { n: 0 }).is_internal());

    registry.shutdown().await.unwrap();
}

/// Coalescing must only drop the collapsing variant: unrelated events queued
/// in the same burst keep their payloads and are not duplicated.
#[tokio::test]
async fn coalescing_preserves_other_events() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 1 });

    registry.send(Burst { n: 1 }).unwrap();
    registry
        .send(Echo {
            msg: "keep".to_string(),
        })
        .unwrap();
    registry.send(Burst { n: 2 }).unwrap();

    let events = drain(&registry).await;

    let bursts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Burst(b) => Some(b.n),
            _ => None,
        })
        .collect();
    assert_eq!(
        bursts,
        vec![2],
        "one coalesced burst, the newest: {events:?}"
    );

    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Echo(e) if e.msg == "keep")),
        "unrelated events survive coalescing: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::EchoResult(r) if r.msg == "echo: keep")),
        "and listeners still run for them: {events:?}"
    );

    registry.shutdown().await.unwrap();
}

/// A request must be answerable without the payload carrying an id.
///
/// The caller owns the continuation, so nothing has to match a `task_id`
/// afterwards and a reply cannot land for the wrong caller.
#[tokio::test]
async fn ask_routes_the_reply_to_its_caller() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 10 });
    let mut reply: ReplyHandle<Answer> = registry.ask(Question { id: 41 }).expect("request queued");

    let answer = tokio::time::timeout(std::time::Duration::from_secs(5), reply.recv())
        .await
        .expect("the listener answered")
        .expect("and the answer was not dropped");

    assert_eq!(answer.id, 41);
    assert_eq!(answer.text, "by multiplier 10");
    assert_eq!(
        registry.pending_replies(),
        0,
        "the reply slot must be released once answered"
    );

    registry.shutdown().await.unwrap();
}

/// Two concurrent asks must not see each other's answers.
#[tokio::test]
async fn concurrent_asks_stay_independent() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 2 });
    let mut first: ReplyHandle<Answer> = registry.ask(Question { id: 1 }).expect("first queued");
    let mut second: ReplyHandle<Answer> = registry.ask(Question { id: 2 }).expect("second queued");
    assert_eq!(registry.pending_replies(), 2, "both in flight");

    assert!(first.try_recv().is_none(), "not answered yet");
    assert!(second.try_recv().is_none(), "not answered yet");

    // Resolve them out of order; each must still get its own answer.
    let second_answer = tokio::time::timeout(std::time::Duration::from_secs(5), second.recv())
        .await
        .expect("second answered")
        .expect("second not dropped");
    let first_answer = tokio::time::timeout(std::time::Duration::from_secs(5), first.recv())
        .await
        .expect("first answered")
        .expect("first not dropped");

    assert_eq!(
        (second_answer.id, second_answer.text.as_str()),
        (2, "by multiplier 2")
    );
    assert_eq!(
        (first_answer.id, first_answer.text.as_str()),
        (1, "by multiplier 2")
    );

    registry.shutdown().await.unwrap();
}

/// Dropping the caller must free its reply slot: otherwise a request nobody
/// waits for leaks registry state forever.
#[tokio::test]
async fn dropping_a_request_frees_its_slot() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 1 });
    let reply: ReplyHandle<Answer> = registry.ask(Question { id: 5 }).expect("queued");
    assert_eq!(registry.pending_replies(), 1);

    drop(reply);

    assert_eq!(
        registry.pending_replies(),
        0,
        "an abandoned request must not leak a slot"
    );

    registry.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_unified_registry() {
    let ctx = AppContext { multiplier: 10 };
    let registry = EventRegistry::spawn(ctx);

    registry
        .send(Echo {
            msg: "Hello".to_string(),
        })
        .unwrap();

    // Verify both the initial event and response event are handled in the registry
    let ev1 = registry.recv().await.unwrap();
    let ev2 = registry.recv().await.unwrap();

    let matches = match (ev1, ev2) {
        (Event::Echo(e), Event::EchoResult(r)) => e.msg == "Hello" && r.msg == "echo: Hello",
        (Event::EchoResult(r), Event::Echo(e)) => e.msg == "Hello" && r.msg == "echo: Hello",
        _ => false,
    };
    assert!(matches);

    registry.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_split_registry() {
    let ctx = AppContext { multiplier: 3 };
    let registry = EventRegistry::spawn(ctx);

    let (sender, receiver, _handle) = registry.into_split();

    sender.send(Math { values: (4, 5) }).unwrap();

    // A failing listener also mirrors a `ListenerFailed`, so the split receiver
    // now carries three events: just require the pair to be among them.
    let events = drain(&receiver).await;
    let has_math = events
        .iter()
        .any(|e| matches!(e, Event::Math(m) if m.values == (4, 5)));
    let has_result = events
        .iter()
        .any(|e| matches!(e, Event::MathResult(r) if r.value == 27));
    assert!(
        has_math && has_result,
        "split receiver must carry both the event and its result: {events:?}"
    );

    sender.shutdown().unwrap();
}

/// A listener that always fails must not stop the dispatch loop: subsequent
/// events still reach their listeners and the mirrored stream keeps flowing.
#[tokio::test]
async fn listener_failure_does_not_stop_dispatch() {
    let ctx = AppContext { multiplier: 3 };
    let registry = EventRegistry::spawn(ctx);

    registry
        .send(Math { values: (4, 5) })
        .expect("first event queued");
    registry
        .send(Math { values: (1, 1) })
        .expect("second event queued");

    // Two results plus two failure reports, in no particular order; what
    // matters is that the loop never stalls and both results still land.
    let events = drain(&registry).await;
    let results = events
        .iter()
        .filter(|e| matches!(e, Event::MathResult(_)))
        .count();
    assert_eq!(
        results, 2,
        "both events must yield results despite the failing listener: {events:?}"
    );

    registry.shutdown().await.unwrap();
}

/// A listener's `Err` must reach the consumer: listeners run detached, so
/// without this the only witness to a failed computation is the log.
#[tokio::test]
async fn listener_failure_is_reported_to_the_consumer() {
    let registry = EventRegistry::spawn(AppContext { multiplier: 1 });
    registry.send(Math { values: (2, 3) }).unwrap();

    // The failing listener runs detached, so its failure and the healthy
    // listener's result race: require both, in any order.
    let events = drain(&registry).await;

    let failure = events.iter().find_map(|e| match e {
        Event::ListenerFailed(failed) => Some(failed),
        _ => None,
    });
    let Some(failed) = failure else {
        panic!("a detached listener's error must still reach the app: {events:?}")
    };
    assert_eq!(failed.event, "Math");
    assert_eq!(failed.listener, "ExplodingListener");
    assert!(
        failed.error.contains("intentional listener failure"),
        "the rendered error must survive: {}",
        failed.error
    );

    assert!(
        events.iter().any(|e| matches!(e, Event::MathResult(_))),
        "sibling listeners still run after one fails: {events:?}"
    );

    registry.shutdown().await.unwrap();
}
