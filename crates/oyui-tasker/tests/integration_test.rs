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

pub struct ExplodingListener;

impl Listener<Math> for ExplodingListener {
    type Sender = EventSender;
    type Context = ();

    async fn handle(_: Math, _: Self::Context, _: EventSender) -> eyre::Result<()> {
        eyre::bail!("intentional listener failure")
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
    ],
    listeners = [
        Echo => [EchoListener],
        Math => [MathListener, ExplodingListener],
    ],
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

    let ev1 = receiver.recv().await.unwrap();
    let ev2 = receiver.recv().await.unwrap();

    let matches = match (ev1, ev2) {
        (Event::Math(m), Event::MathResult(r)) => m.values == (4, 5) && r.value == 27,
        (Event::MathResult(r), Event::Math(m)) => m.values == (4, 5) && r.value == 27,
        _ => false,
    };
    assert!(matches);

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

    // Two input echoes plus two results, in no particular order.
    let mut results = 0;
    for _ in 0..4 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), registry.recv())
            .await
            .expect("dispatch loop stalled after a listener error")
            .expect("registry closed unexpectedly");
        if matches!(event, Event::MathResult(_)) {
            results += 1;
        }
    }
    assert_eq!(
        results, 2,
        "both events must yield results despite the failing listener"
    );

    registry.shutdown().await.unwrap();
}
