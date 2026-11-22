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

`EventReceiver::recv()` and `try_recv()` transparently replace a coalescing
signal with that class's newest payload; callers normally should not handle a
signal themselves. `take_coalesced(slot)` is a lower-level escape hatch:
`slot` is the zero-based position of a variant in the `collapse` list, in
declaration order. It takes the current payload and re-arms that class; `None`
means the slot is empty or the index is invalid. Prefer `recv()` / `try_recv()`
unless you are deliberately managing coalesced payloads yourself.

`EventReceiver::len()` and `EventRegistry::pending()` count mirrored queue
entries, not events still being handled by detached listener tasks. A collapsing
burst contributes at most one queued wake-up per class; its payload is held in
the coalescing slot. `is_empty()` has the same queue-only meaning.

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

`try_recv()` returns `None` while the answer is not ready and leaves the reply
slot registered so it can be polled again. Once it returns `Some(answer)`, the
handle has consumed its receiver; drop the handle to release the slot. In
contrast, `recv().await` waits and releases the slot when it completes. Dropping
either handle before an answer arrives cancels the wait and removes the slot;
a later listener reply then fails as undeliverable.

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

`into_split()` returns `(EventSender, EventReceiver, Option<JoinHandle<()>>)`.
The split sender can send events and enqueue `Shutdown`, but cannot call
`ask()`; reply requests must be initiated through the owning `EventRegistry`.
To stop a split registry, call `sender.shutdown()` and await the returned join
handle when it is `Some`. With the unsplit registry, `registry.shutdown().await`
both queues shutdown and waits for the dispatch task. Sending or shutting down
after the dispatch channel has closed returns an error. `pending()` and receiver
`len()` / `is_empty()` describe only the mirrored queue; they do not indicate
whether a request has an answer. Use `pending_replies()` for outstanding reply
slots.

### Context derive constraints

Both derives accept only structs with named fields (not tuple/unit structs,
enums, or unions), and generic structs are not currently supported.
`TaskerProvide` generates an extraction for each field by cloning it from the
registry context, so every provided field must implement `Clone`.
`TaskerContext` builds a listener context by extracting each field type from
the registry context; each field type must implement
`ExtractsFrom<RegistryContext>` (usually via `TaskerProvide` or another
`TaskerContext` derive). Field names need not match: extraction is based on
field types.

### Lower-level coalescing cell

`CoalesceCell<T>` is the standalone primitive used by generated `collapse`
routes. `store(value)` overwrites any older pending value and returns `true`
only for the store that changes the cell from empty to occupied; that return
value is intended to drive one wake-up. `take()` returns the newest value and
re-arms the cell for another wake-up. `generation()` counts stores and can help
a consumer avoid redundant drains; it is not a count of pending values.
Ordinary registry users should use `collapse` and the generated receiver
instead of managing cells directly.
