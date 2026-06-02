# oyui-rune-actions

`oyui-rune-actions` is a declarative, macro-driven framework designed to define nested action hierarchies, generate corresponding Rust handler traits, and register them as modules within the [Rune scripting language](https://rune-rs.github.io/). 

By defining your action tree once, the library produces the necessary structures for both type-safe Rust dispatch and runtime scripting.

[Check the official repository for more information](https://github.com/emilien-jegou/oyui)

---

## Features

- **Hierarchical DSL**: Define nested structures using a simple syntax representing namespaces, functions, and properties.
- **Rune Scripting Integration**: Automatic generation of `rune::Module` configurations matching your nested structures (e.g. `view::file::cursor::up()`).
- **Rust Trait Generation**: Segmented handler traits for each nested path, enabling modular or "all-in-one" implementations.
- **Type-Erased Dispatching**: A compiled, non-generic `BoxedHandler` providing a `.dispatch(&Action)` interface for runtime-received events.
- **Get/Set Properties**: Built-in `@getset(Type)` helper to easily expose fields with getter and setter methods.

---

## Quick Start

### 1. Define the Actions Hierarchy

Use the `define_actions!` macro to describe the nested structure.

```rust
use oyui_rune_actions::define_actions;

define_actions! {
  view {
     file {
       scroll {
         left(u32),
         right(u32),
       },
       cursor {
         up(),
         down(u32),
       }
     }
  },
  system {
     theme(|String| -> String),
     config(|| -> u32),
     fg { @getset(String) }
  }
}
```

The macro generates:
- An outer structural enum `Actions` representing the AST hierarchy.
- Helper traits such as `ViewFileScrollActionsHandler`, `ViewFileCursorActionsHandler`, `SystemActionsHandler`, and `SystemFgActionsHandler`.
- A generic `Handler` struct and a type-erased `BoxedHandler` wrapper.
- A `register_actions` function to load the entire interface into Rune.

### 2. Implement the Trait Handlers

You can implement these handlers across multiple modular structs or combine them into a single type.

```rust
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};

struct CursorHandler {
    cursor_down_val: Arc<AtomicU32>,
}

impl ViewFileCursorActionsHandler for CursorHandler {
    fn up(&self) {
        println!("Cursor went up");
    }
    fn down(&self, val: u32) {
        self.cursor_down_val.store(val, Ordering::SeqCst);
    }
}

struct SystemFgHandler {
    fg: Mutex<String>,
}

impl SystemFgActionsHandler for SystemFgHandler {
    fn get(&self) -> String {
        self.fg.lock().unwrap().clone()
    }
    fn set(&self, val: String) {
        *self.fg.lock().unwrap() = val;
    }
}
```

### 3. Registering with Rune

Instantiate the generated `Handler`, call `.build()` to obtain a type-erased `BoxedHandler`, and register the module definitions with your Rune `Context`.

```rust
use rune::{Context, Diagnostics, Source, Sources, Vm};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = Context::with_default_modules()?;
    
    // Stub implementations for the remaining handlers
    struct DummyScroll;
    impl ViewFileScrollActionsHandler for DummyScroll {
        fn left(&self, _: u32) {}
        fn right(&self, _: u32) {}
    }
    struct DummySystem;
    impl SystemActionsHandler for DummySystem {
        fn theme(&self, val: String) -> String { val }
        fn config(&self) -> u32 { 100 }
    }

    let handler = Handler {
        view_file_scroll: DummyScroll,
        view_file_cursor: CursorHandler {
            cursor_down_val: Arc::new(AtomicU32::new(0)),
        },
        system: DummySystem,
        system_fg: SystemFgHandler {
            fg: Mutex::new("blue".to_string()),
        },
    }.build();

    // Register handlers to the Rune context
    register_actions(&mut context, handler)?;

    let runtime = Arc::new(context.runtime()?);

    // Call registered modules within Rune
    let mut sources = Sources::new();
    sources.insert(Source::new(
        "script",
        r#"
        pub fn run() {
            view::file::cursor::up();
            view::file::cursor::down(42);
            system::fg::set("red");
            system::fg::get()
        }
        "#,
    )?)?;

    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    let unit = result?;
    let mut vm = Vm::new(runtime, Arc::new(unit));

    let value = vm.call(["run"], ())?;
    let color: String = rune::from_value(value)?;
    assert_eq!(color, "red");

    Ok(())
}
```

### 4. Direct Dispatching in Rust

If you receive serialized or runtime-constructed `Action` objects, you can route them synchronously using `.dispatch()`:

```rust
let action_down: Action = ViewFileCursorActions::down(456).into();

// Dispatch dynamically on the erased handler
handler.dispatch(&action_down);
```

---

## DSL Syntax Specifications

The nested syntax inside the `define_actions!` macro supports several structural representations:

### Branches and Namespaces
A branch block generates nested sub-modules in Rune and namespace-bound enums in Rust.
```rust
parent_namespace {
    child_namespace {
        // ...
    }
}
```

### Void Leaf Nodes
Methods that require no arguments and return nothing (`()`) are written with empty parentheses:
```rust
up()
```

### Value-Argument Leaf Nodes
Methods that take simple types are specified directly:
```rust
down(u32)
```

### Functions with Arguments and Return Types
Use the closure-like notation `(|ArgType, ...| -> ReturnType)` to specify return constraints:
```rust
theme(|String| -> String)
```

### State Getters and Setters
Use the `@getset(Type)` keyword inside a branch. It generates a stateful property containing a `get() -> Type` and `set(Type)` pair:
```rust
fg { @getset(String) }
```
