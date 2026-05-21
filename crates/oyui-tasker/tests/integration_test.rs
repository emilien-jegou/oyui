use oyui_tasker::register_tasker;
use oyui_tasker::worker::{ExtractsFrom, WorkerTask};

pub struct EchoTask;

impl WorkerTask for EchoTask {
    type Request = String;
    type Response = String;
    type Context = ();

    async fn handle(req: Self::Request, _ctx: Self::Context) -> Self::Response {
        format!("echo: {}", req)
    }
}

pub struct MathTask;

impl WorkerTask for MathTask {
    type Request = (i32, i32);
    type Response = i32;
    type Context = i32;

    async fn handle(req: Self::Request, ctx: Self::Context) -> Self::Response {
        (req.0 + req.1) * ctx
    }
}

#[derive(Clone)]
pub struct AppContext {
    pub multiplier: i32,
}

impl ExtractsFrom<AppContext> for i32 {
    fn extract(ctx: &AppContext) -> Self {
        ctx.multiplier
    }
}

register_tasker! {
    tasks = [
        Echo => EchoTask,
        Math => MathTask,
    ]
}

#[tokio::test]
async fn test_unified_tasker() {
    let ctx = AppContext { multiplier: 10 };
    let mut tasker = Tasker::spawn(ctx);

    // Test Echo Task
    tasker.send(String::from("Hello")).unwrap();
    let ev = tasker.recv().await.unwrap();

    if let WorkerEvent::Echo(res) = ev {
        assert_eq!(res, "echo: Hello");
    } else {
        panic!("Expected Echo event");
    }

    // Test Math Task
    tasker.send((2, 3)).unwrap();
    let ev = tasker.recv().await.unwrap();

    if let WorkerEvent::Math(res) = ev {
        assert_eq!(res, 50);
    } else {
        panic!("Expected Math event");
    }

    tasker.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_split_tasker() {
    let ctx = AppContext { multiplier: 2 };
    let tasker = Tasker::spawn(ctx);

    // Using into_split because MPSC Receiver is not cloneable
    let (sender, mut receiver, _) = tasker.into_split();

    sender.send(String::from("From Thread")).unwrap();
    sender.send((10, 10)).unwrap();

    // Directly await events
    let ev1 = receiver.recv().await.unwrap();
    let ev2 = receiver.recv().await.unwrap();

    match (ev1, ev2) {
        (WorkerEvent::Echo(r1), WorkerEvent::Math(r2)) => {
            assert_eq!(r1, "echo: From Thread");
            assert_eq!(r2, 40);
        }
        _ => panic!("Expected correct sequence of events"),
    }
}

#[tokio::test]
async fn test_shutdown_cleans_up() {
    let ctx = AppContext { multiplier: 1 };
    let mut tasker = Tasker::spawn(ctx);

    tasker.shutdown().await.expect("Shutdown failed");

    // Sending should fail because the receiver is dropped in the background loop
    let send_result = tasker.send(String::from("Should fail"));
    assert!(send_result.is_err());
}

#[tokio::test]
async fn test_into_split_consumes_tasker() {
    let ctx = AppContext { multiplier: 1 };
    let tasker = Tasker::spawn(ctx);

    let (sender, mut receiver, handle) = tasker.into_split();

    sender.send(String::from("IntoSplit")).unwrap();
    let ev = receiver.recv().await.unwrap();

    if let WorkerEvent::Echo(res) = ev {
        assert_eq!(res, "echo: IntoSplit");
    }

    sender.shutdown().unwrap();
    handle.unwrap().await.unwrap();
}
