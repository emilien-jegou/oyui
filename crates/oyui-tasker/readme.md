# oyui-tasker

`oyui-tasker` is a macro-driven event distribution and background task execution library for Rust. It helps orchestrate asynchronous event loops, route events to registered listeners, and manage context sharing across those listeners using `tokio` tasks.

[Check the official repository for more information](https://github.com/emilien-jegou/oyui)

## Features

* **Macro-Driven Registry**: Define your events and associate them with listeners in a declarative way using the `tasker_registry!` macro.
* **Context Extraction**: Share application context with listeners safely. Implement context extraction automatically using `#[derive(TaskerProvide)]` and `#[derive(TaskerContext)]`.
* **Flexible Dispatching**: Use `EventRegistry` in a unified manner or split it into an `EventSender` and `EventReceiver` pair for separate storage and thread-safety.
* **Tracing Support**: Out-of-the-box integration with `tracing` to instrument listener execution and trace failures.

---

## Usage

Below is an overview of how to define events, create listeners, manage context, and spin up the registry.

### 1. Define Events

Events can be any type that implements `Clone`, `Send`, `Sync`, and `'static`.

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Echo {
    pub msg: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EchoResult {
    pub msg: String,
}
```

### 2. Implement the `Listener` Trait

Implement the `Listener` trait for your handlers. Each listener defines its expected `Context` and performs asynchronous work inside the `handle` function.

```rust
use oyui_tasker::{Listener, EventSender};

pub struct EchoListener;

impl Listener<Echo, EventSender> for EchoListener {
    // The specific context type this listener requires.
    type Context = ();

    async fn handle(event: Echo, _ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // Send a result back into the system if needed
        tx.send(EchoResult {
            msg: format!("echo: {}", event.msg),
        })?;
        Ok(())
    }
}
```

### 3. Setup Context Extraction

The context required by listeners is extracted from a global application context. You can derive `TaskerProvide` on your application context to allow individual fields to be extracted.

```rust
use oyui_tasker::TaskerProvide;

#[derive(Clone, TaskerProvide)]
pub struct AppContext {
    pub multiplier: i32,
}
```

If a listener requires a subset of fields packed into a specific struct, you can use `#[derive(TaskerContext)]` to construct that struct from the global context automatically:

```rust
use oyui_tasker::TaskerContext;

#[derive(TaskerContext)]
pub struct SubContext {
    pub multiplier: i32,
}
```

### 4. Declare the Registry

Use the `tasker_registry!` macro to bind everything together. This macro generates:
- An `Event` enum wrapping all specified variants plus a `Shutdown` variant.
- An `EventSender` and an `EventReceiver`.
- An `EventRegistry` coordinator.

```rust
use oyui_tasker::tasker_registry;

tasker_registry! {
    events = [
        Echo       => Echo,
        EchoResult => EchoResult,
    ],
    listeners = [
        Echo => [EchoListener],
    ],
}
```

### 5. Running the Registry

You can run the registry in a unified loop or split it into a sender and receiver.

#### Unified Usage

```rust
#[tokio::main]
async fn main() {
    let ctx = AppContext { multiplier: 10 };
    let mut registry = EventRegistry::spawn(ctx);

    // Send an event
    registry.send(Echo { msg: "Hello".to_string() }).unwrap();

    // Read events coming out of the loop
    if let Some(event) = registry.recv().await {
        println!("Received event: {:?}", event);
    }

    // Cleanly shutdown
    registry.shutdown().await.unwrap();
}
```

#### Split Usage

If you need to move the sender and receiver to different contexts or threads, split the registry:

```rust
#[tokio::main]
async fn main() {
    let ctx = AppContext { multiplier: 10 };
    let registry = EventRegistry::spawn(ctx);

    let (sender, mut receiver, _join_handle) = registry.into_split();

    // Send asynchronously from elsewhere
    sender.send(Echo { msg: "Hello".to_string() }).unwrap();

    // Receive events
    while let Some(event) = receiver.recv().await {
        println!("Received event: {:?}", event);
    }
}
```
