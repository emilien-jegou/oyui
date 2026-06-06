use crate::{actions::BoxedHandler, config::LineHighlightMode};
use rune::{termcolor, Context, ContextError, Module, Vm};
use std::rc::Rc;
use std::sync::Arc;
use tracing::{debug, error, info, info_span};

thread_local! {
    pub static CURRENT_COMPILING_MODE: std::cell::RefCell<Option<crate::actions::keybinds::KeybindMode>> =
        const { std::cell::RefCell::new(None) };
}

pub fn base_module() -> Result<Module, ContextError> {
    let mut m = Module::new();
    m.ty::<LineHighlightMode>()?;
    Ok(m)
}

pub fn build_context(handler: BoxedHandler) -> Result<Context, ContextError> {
    let mut context = rune::Context::with_default_modules()?;
    context.install(base_module()?)?;

    let mut m = Module::new();

    // Register the global `keybind` function that accepts a string + closure
    m.function("keybind", |kb_str: String, cb: rune::runtime::Function| {
        let kb = crate::commons::input::Keybind::parse(&kb_str);

        let mode_opt = CURRENT_COMPILING_MODE.with(|m| m.borrow().clone());

        crate::config::ACTIVE_REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            let owned_reg =
                std::mem::take(&mut *reg);

            if let Some(mode) = mode_opt {
                *reg = owned_reg.register_fn_mode(mode, kb, Rc::new(cb));
            } else {
                *reg = owned_reg.register_fn(kb, Rc::new(cb));
            }
        });
    })
    .build()?;

    // Register the dynamic `on_mode` function to contextually nest keybinds
    m.function(
        "on_mode",
        |mode_str: String, cb: rune::runtime::Function| {
            let mode = match mode_str.to_lowercase().as_str() {
                "file" => crate::actions::keybinds::KeybindMode::View(
                    crate::actions::keybinds::View::File,
                ),
                "tree" => crate::actions::keybinds::KeybindMode::View(
                    crate::actions::keybinds::View::Tree,
                ),
                _ => {
                    return rune::runtime::VmResult::Ok(());
                }
            };

            CURRENT_COMPILING_MODE.with(|m| *m.borrow_mut() = Some(mode));
            let result = cb.call::<()>(());
            CURRENT_COMPILING_MODE.with(|m| *m.borrow_mut() = None);
            result
        },
    )
    .build()?;

    context.install(m)?;
    crate::actions::register_actions(&mut context, handler)?;
    Ok(context)
}

/// Compile and return a ready [`Vm`] from a `.rn` source file.
pub fn build_vm(
    path: &std::path::Path,
    handler: BoxedHandler,
) -> Result<Vm, Box<dyn std::error::Error>> {
    let span = info_span!("build_vm", path = %path.display());
    let _enter = span.enter();

    debug!("Registering runtime context and modules");

    let context = build_context(handler)?;

    let runtime = Arc::new(context.runtime()?);

    debug!("Parsing source file");
    let source = rune::Source::from_path(path)?;
    let mut sources = rune::Sources::new();
    sources.insert(source)?;

    debug!("Compiling source file to VM bytecode");
    let mut diagnostics = rune::Diagnostics::new();

    let unit = match rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()
    {
        Ok(unit) => unit,
        Err(e) => {
            let mut buffer = termcolor::Buffer::ansi();
            if let Err(emit_err) = diagnostics.emit(&mut buffer, &sources) {
                error!("Failed to emit compilation diagnostics to buffer: {emit_err}");
            }

            let error_msg = String::from_utf8_lossy(&buffer.into_inner()).into_owned();
            let full_error = if error_msg.trim().is_empty() {
                e.to_string()
            } else {
                error_msg
            };

            error!(error = %full_error, "Compiler syntax or build error in Rune script");
            return Err(full_error.into());
        }
    };

    debug!("VM compilation succeeded");
    Ok(Vm::new(runtime, Arc::new(unit)))
}

/// Call the script's `config()` function and return the queue of parsed `Action` items.
pub fn run_config_script(vm: &mut Vm) -> Result<(), Box<dyn std::error::Error>> {
    let span = info_span!("run_config_script");
    let _enter = span.enter();

    match vm.call(["config"], ()) {
        Ok(_) => {
            info!("Successfully executed 'config' script function");
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("Missing function") {
                debug!("Function 'config' is absent.");
                Ok(())
            } else {
                error!("Runtime error while running 'config' function: {e}");
                Err(e.into())
            }
        }
    }
}
