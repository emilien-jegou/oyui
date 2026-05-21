# Oyui tasker

A way to manage async work in oyui via threaded background workers. Tasks are executed concurrently on Tokio's worker pool, and communication is handled smoothly via lock-free channels.

## Usage

### 0. Setup your App Context
Your global application state can use `TaskerProvide` to automatically implement extraction rules for its fields.

```rust
use oyui_tasker::TaskerProvide;

#[derive(Clone, Debug)]
pub struct MyName(String);

// `TaskerProvide` automatically implements `ExtractsFrom<AppContext>` for the `MyName` type.
#[derive(TaskerProvide, Clone)]
pub struct AppContext {
    name: MyName
}
```

### 1. Define your tasks
Implement `WorkerTask`. A task defines what it needs as a Request, what it returns as a Response, and exactly what it needs from the context.

```rust
use oyui_tasker::WorkerTask;

pub struct StatsTask;

#[derive(Clone, Debug)]
pub struct StatsReq { pub path: String }
pub struct StatsRes { pub name: String, pub path: String, pub size: u64 }

impl WorkerTask for StatsTask {
    type Request = StatsReq;
    type Response = StatsRes;
    type Context = MyName;

    async fn handle(req: Self::Request, ctx: Self::Context) -> Self::Response {
        StatsRes { name: ctx.0.clone(), path: req.path, size: 42 }
    }
}
```

### 2. Register the worker
Use the `register_tasker!` macro to generate the required background worker, channels, and enums.

```rust
use oyui_tasker::register_tasker;

register_tasker! {
    tasks = [
        Stats => StatsTask,
    ]
}
```

### 3. Spawn and Interact!
Everything runs through the unified `Tasker` object.

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = AppContext {
        name: MyName("John".into())
    };

    // 1. Spawn the worker pool
    let mut worker = Tasker::spawn(ctx);

    // 2. Send a request
    worker.send(StatsReq { path: "/tmp".to_string() }).unwrap();

    // 3. Receive a response (usually done in your app's event/tick loop)
    while let Ok(event) = worker.try_recv() {
        match event {
            WorkerEvent::Stats(res) => {
                println!("Got stats for {}: {}", res.path, res.size);
            }
        }
    }

    // 4. Shutdown gracefully
    worker.shutdown().await?;
    
    Ok(())
}
```

---

## Advanced Usage

### Tasks with Multiple Dependencies (`TaskerContext`)
If a task requires multiple pieces of context (e.g., a Database connection *and* a Config file), you shouldn't tie it directly to your global `AppContext`. Instead, define a local struct using `#[derive(TaskerContext)]`. 

It will automatically assemble itself from the global context!

```rust
use oyui_tasker::TaskerContext;

// 1. Define exactly what this specific task needs
#[derive(TaskerContext)]
pub struct StatContext {
    firstname: MyFirstName,
    lastname: MyLastName,
}

// 2. Use it in the task
impl WorkerTask for StatsTask {
    type Request = StatsReq;
    type Response = StatsRes;
    type Context = StatContext;

    async fn handle(req: Self::Request, ctx: Self::Context) -> Self::Response {
        let full_name = format!("{} {}", ctx.firstname.0, ctx.lastname.0);
        StatsRes { name: full_name, path: req.path, size: 42 }
    }
}
```

### Splitting the Tasker
If you have a UI thread reading events, and multiple other background threads sending requests, you can split the `Tasker`:

```rust
// Extracts lightweight Sender and Receiver handles
let (sender, receiver, _handle) = worker.into_split();

// `sender` and `receiver` can be safely cloned and moved across threads.
let sender_clone = sender.clone();
std::thread::spawn(move || {
    sender_clone.send(StatsReq { path: "/usr".into() }).unwrap();
});
```
