use oyui_rune_actions::define_actions; // Handler and BoxedHandler are generated locally by the macro
use rune::{Context, Diagnostics, Source, Sources, Vm};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

static EXECUTED: AtomicU32 = AtomicU32::new(0);
static CURSOR_UP_CALLED: AtomicBool = AtomicBool::new(false);
static CURSOR_DOWN_VAL: AtomicU32 = AtomicU32::new(0);

// Invoke the macro to generate structures and split traits for testing
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

struct MyScrollHandler {
    executed: Arc<AtomicU32>,
}
impl ViewFileScrollActionsHandler for MyScrollHandler {
    fn left(&self, val: u32) {
        self.executed.store(val, Ordering::SeqCst);
    }
    fn right(&self, _val: u32) {}
}

struct MyCursorHandler {
    cursor_up_called: Arc<AtomicBool>,
    cursor_down_val: Arc<AtomicU32>,
}
impl ViewFileCursorActionsHandler for MyCursorHandler {
    fn up(&self) {
        self.cursor_up_called.store(true, Ordering::SeqCst);
    }
    fn down(&self, val: u32) {
        self.cursor_down_val.store(val, Ordering::SeqCst);
    }
}

struct MySystemHandler;
impl SystemActionsHandler for MySystemHandler {
    fn theme(&self, val: String) -> String {
        val
    }
    fn config(&self) -> u32 {
        42
    }
}

struct MySystemFgHandler {
    fg: Mutex<String>,
}
impl SystemFgActionsHandler for MySystemFgHandler {
    fn get(&self) -> String {
        self.fg.lock().unwrap().clone()
    }
    fn set(&self, val: String) {
        *self.fg.lock().unwrap() = val;
    }
}


struct AllInOneHandler {
    fg: Mutex<String>,
    executed: Arc<AtomicU32>,
    cursor_up_called: Arc<AtomicBool>,
    cursor_down_val: Arc<AtomicU32>,
}

impl ViewFileScrollActionsHandler for AllInOneHandler {
    fn left(&self, val: u32) {
        self.executed.store(val, Ordering::SeqCst);
    }
    fn right(&self, _val: u32) {}
}

impl ViewFileCursorActionsHandler for AllInOneHandler {
    fn up(&self) {
        self.cursor_up_called.store(true, Ordering::SeqCst);
    }
    fn down(&self, val: u32) {
        self.cursor_down_val.store(val, Ordering::SeqCst);
    }
}

impl SystemFgActionsHandler for AllInOneHandler {
    fn get(&self) -> String {
        self.fg.lock().unwrap().clone()
    }
    fn set(&self, val: String) {
        *self.fg.lock().unwrap() = val;
    }
}

impl SystemActionsHandler for AllInOneHandler {
    fn theme(&self, val: String) -> String {
        format!("{}-custom-theme", val)
    }
    fn config(&self) -> u32 {
        100
    }
}

#[test]
fn test_enum_structural_generation() {
    let action = Actions::view(ViewActions::file(ViewFileActions::scroll(
        ViewFileScrollActions::left(42),
    )));

    match action {
        Actions::view(ViewActions::file(ViewFileActions::scroll(ViewFileScrollActions::left(
            val,
        )))) => {
            assert_eq!(val, 42);
        }
        _ => panic!("Enum structure match failed"),
    }
}

#[test]
fn test_direct_rust_modular_handler_execution() {
    let executed = Arc::new(AtomicU32::new(0));
    let cursor_up_called = Arc::new(AtomicBool::new(false));
    let cursor_down_val = Arc::new(AtomicU32::new(0));

    let handler = Handler {
        view_file_scroll: MyScrollHandler { executed: executed.clone() },
        view_file_cursor: MyCursorHandler { 
            cursor_up_called: cursor_up_called.clone(), 
            cursor_down_val: cursor_down_val.clone() 
        },
        system: MySystemHandler,
        system_fg: MySystemFgHandler {
            fg: Mutex::new(String::from("blue")),
        },
    };

    handler.view_file_scroll.left(99);
    assert_eq!(executed.load(Ordering::SeqCst), 99);

    handler.view_file_cursor.up();
    assert!(cursor_up_called.load(Ordering::SeqCst));

    handler.view_file_cursor.down(55);
    assert_eq!(cursor_down_val.load(Ordering::SeqCst), 55);

    assert_eq!(handler.system.theme(String::from("light")), "light");
    assert_eq!(handler.system.config(), 42);
    assert_eq!(handler.system_fg.get(), "blue");

    handler.system_fg.set(String::from("green"));
    assert_eq!(handler.system_fg.get(), "green");
}

#[test]
fn test_rune_modular_module_execution() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = Context::with_default_modules()?;

    let executed = Arc::new(AtomicU32::new(0));
    let cursor_up_called = Arc::new(AtomicBool::new(false));
    let cursor_down_val = Arc::new(AtomicU32::new(0));

    let handler = Handler {
        view_file_scroll: MyScrollHandler { executed: executed.clone() },
        view_file_cursor: MyCursorHandler { 
            cursor_up_called: cursor_up_called.clone(), 
            cursor_down_val: cursor_down_val.clone() 
        },
        system: MySystemHandler,
        system_fg: MySystemFgHandler {
            fg: Mutex::new(String::new()),
        },
    }.build();

    register_actions(&mut context, handler)?;

    let runtime = Arc::new(context.runtime()?);

    let mut sources = Sources::new();
    sources.insert(Source::new(
        "test_script_modular",
        r#"
        pub fn test() {
            view::file::cursor::up(); 
            view::file::cursor::down(15);
            view::file::scroll::left(100);
            let t = system::theme("dark");
            let c = system::config();
            system::fg::set("red");
            let f = system::fg::get();
            (t, c, f)
        }
        "#,
    )?)?;

    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        println!("Diagnostics: {:?}", diagnostics);
    }

    let unit = result?;
    let mut vm = Vm::new(runtime, Arc::new(unit));

    let value = vm.call(["test"], ())?;

    let (t, c, f): (String, u32, String) = rune::from_value(value)?;

    assert_eq!(t, "dark");
    assert_eq!(c, 42);
    assert_eq!(f, "red");
    assert_eq!(executed.load(Ordering::SeqCst), 100);
    assert!(cursor_up_called.load(Ordering::SeqCst));
    assert_eq!(cursor_down_val.load(Ordering::SeqCst), 15);

    Ok(())
}

#[test]
fn test_rune_all_in_one_module_execution() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = Context::with_default_modules()?;

    let executed = Arc::new(AtomicU32::new(0));
    let cursor_up_called = Arc::new(AtomicBool::new(false));
    let cursor_down_val = Arc::new(AtomicU32::new(0));

    let all_in_one = Arc::new(AllInOneHandler {
        fg: Mutex::new(String::from("initial_yellow")),
        executed: executed.clone(),
        cursor_up_called: cursor_up_called.clone(),
        cursor_down_val: cursor_down_val.clone(),
    });

    let handler = Handler {
        view_file_scroll: all_in_one.clone(),
        view_file_cursor: all_in_one.clone(),
        system: all_in_one.clone(),
        system_fg: all_in_one,
    }.build();

    register_actions(&mut context, handler)?;

    let runtime = Arc::new(context.runtime()?);

    let mut sources = Sources::new();
    sources.insert(Source::new(
        "test_script_flat",
        r#"
        pub fn test() {
            view::file::cursor::up(); 
            view::file::cursor::down(88);
            view::file::scroll::left(250);
            let t = system::theme("light");
            let c = system::config();
            let initial = system::fg::get();
            system::fg::set("purple");
            let modified = system::fg::get();
            (t, c, initial, modified)
        }
        "#,
    )?)?;

    let mut diagnostics = Diagnostics::new();
    let result = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build();

    if !diagnostics.is_empty() {
        println!("Diagnostics: {:?}", diagnostics);
    }

    let unit = result?;
    let mut vm = Vm::new(runtime, Arc::new(unit));

    let value = vm.call(["test"], ())?;

    let (t, c, initial, modified): (String, u32, String, String) = rune::from_value(value)?;

    assert_eq!(t, "light-custom-theme");
    assert_eq!(c, 100);
    assert_eq!(initial, "initial_yellow");
    assert_eq!(modified, "purple");
    assert_eq!(executed.load(Ordering::SeqCst), 250);
    assert!(cursor_up_called.load(Ordering::SeqCst));
    assert_eq!(cursor_down_val.load(Ordering::SeqCst), 88);

    Ok(())
}
