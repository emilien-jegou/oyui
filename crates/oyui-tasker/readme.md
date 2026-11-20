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

impl Listener<Echo> for EchoListener {
    // The sender type: the registry's generated `EventSender`.
    type Sender = EventSender;
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

Two optional lists route each variant to the app:

* `internal` — dispatched to listeners, never mirrored to the app receiver.
* `collapse` — mirrored, but a burst leaves only the newest pending copy.

```rust
use oyui_tasker::tasker_registry;

tasker_registry! {
    events = [
        Echo       => Echo,
        EchoResult => EchoResult,
        Stats      => StatsReq,
        StatsRes   => StatsRes,
    ],
    // The request never needs to wake the app; the result does.
    internal = [Stats],
    // A burst of per-file results only needs one repaint.
    collapse = [StatsRes],
    listeners = [
        Echo       => [EchoListener],
        Stats      => [StatsListener],
        StatsRes   => [StatsListener],
    ],
}
```

#### Why routing matters

Every mirrored event wakes whatever is consuming `EventReceiver`, and a
consumer that repaints on wakeup turns one event into one repaint. Worker
plumbing (`StatsReq`) and bursty per-file results (`StatsRes`) are exactly the
two shapes you do not want to pay a repaint for, and they are the common case:
`internal` stops the plumbing from reaching the consumer at all, and `collapse`
turns a hundred results into one pending signal while the consumer drains what
it already has.

Both are opt-in — with neither list, every event is mirrored exactly as before.

`Event::is_internal()` and `Event::is_collapsing()` expose the classification,
and `EventRegistry::pending()` reports how many mirrored events are waiting,
which is a cheap way to confirm a burst is being coalesced.

#### Listener failures

Listeners run as detached tasks, so their `Err` has no caller to propagate to.
The registry mirrors it as an extra variant instead:

```rust
match event {
    Event::ListenerFailed(failed) => show(failed.to_string()),
    _ => {}
}
```

This is what turns a background computation that silently did not happen into
something a UI can explain.

### 6. Requests with a named answer

Broadcast is right for state a listener publishes and wrong for an answer one
caller asked for. A declared `replies` pairing gives that request a private
answer:

```rust
tasker_registry! {
    events = [
        Ask    => Asked<Question>,
        Answer => Answer,
    ],
    replies = [
        Ask => Answer,
    ],
    listeners = [
        Ask => [AnsweringListener],
    ],
}

impl Listener<Question> for AnsweringListener {
    type Sender = EventSender;
    type Context = ();

    async fn handle(event: Question, _ctx: (), tx: EventSender) -> eyre::Result<()> {
        tx.reply(Answer { text: format!("got {}", event.id) })?;
        Ok(())
    }
}
```

The caller holds the continuation:

```rust
let mut reply: Reply<Answer> = registry.ask(Question { id: 1 })?;
let answer = reply.recv().await.expect("answer arrived");
```

**Why it beats an id in the payload.** The payload needs no `task_id`, so the
listener never learns which caller asked and no one has to match ids after the
fact. The pairing is checked at compile time (`R: ReplyOf<E>`), a request that
nobody answers cannot be sent, and dropping the caller releases its reply slot
— a lost request cannot leak registry state.

`try_recv` lets a single-threaded consumer resolve replies alongside its other
work, which is what a UI loop needs when the answer must be delivered on the
thread that asked.

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
