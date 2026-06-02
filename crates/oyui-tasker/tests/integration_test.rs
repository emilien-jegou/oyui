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

impl Listener<Echo, EventSender> for EchoListener {
    type Context = ();

    async fn handle(event: Echo, _ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tx.send(EchoResult {
            msg: format!("echo: {}", event.msg),
        })?;
        Ok(())
    }
}

pub struct MathListener;

impl Listener<Math, EventSender> for MathListener {
    type Context = i32;

    async fn handle(event: Math, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tx.send(MathResult {
            value: (event.values.0 + event.values.1) * ctx,
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
    ],
    listeners = [
        Echo => [EchoListener],
        Math => [MathListener],
    ],
}

#[tokio::test]
async fn test_unified_registry() {
    let ctx = AppContext { multiplier: 10 };
    let mut registry = EventRegistry::spawn(ctx);

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

    let (sender, mut receiver, _handle) = registry.into_split();

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
