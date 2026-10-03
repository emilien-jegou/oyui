//! Compilation and entrypoint dispatch for `.rn` config scripts.

use super::ScriptError;
use rune::{termcolor, Context, Vm};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, info_span};

/// Compiles `path` against `context` into a ready [`Vm`].
pub(super) fn build_vm(path: &Path, context: Context) -> Result<Vm, ScriptError> {
    let span = info_span!("build_vm", path = %path.display());
    let _enter = span.enter();

    debug!("Registering runtime context and modules");
    let runtime = Arc::new(
        context
            .runtime()
            .map_err(|e| ScriptError::new(format!("Failed to build runtime: {e}")))?,
    );

    debug!("Parsing source file");
    let source = rune::Source::from_path(path)
        .map_err(|e| ScriptError::new(format!("Failed to read {}: {e}", path.display())))?;
    let mut sources = rune::Sources::new();
    sources
        .insert(source)
        .map_err(|e| ScriptError::new(format!("Failed to register source: {e}")))?;

    debug!("Compiling source file to VM bytecode");
    let mut diagnostics = rune::Diagnostics::new();

    let unit = match rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()
    {
        Ok(unit) => unit,
        Err(e) => {
            let message = render_diagnostics(&diagnostics, &sources, e.to_string());
            error!(error = %message, "Compiler syntax or build error in Rune script");
            return Err(ScriptError::new(message));
        }
    };

    debug!("VM compilation succeeded");
    Ok(Vm::new(runtime, Arc::new(unit)))
}

/// Runs the script's optional `config()` entrypoint.
///
/// A script may legitimately only wire up keybinds, so a missing entrypoint
/// is not an error.
pub(super) fn run_config_script(vm: &mut Vm) -> Result<(), ScriptError> {
    let span = info_span!("run_config_script");
    let _enter = span.enter();

    let config_fn = match vm.lookup_function(["config"]) {
        Ok(config_fn) => config_fn,
        Err(_) => {
            debug!("Function 'config' is absent.");
            return Ok(());
        }
    };

    match config_fn.call::<()>(()).into_result() {
        Ok(()) => {
            info!("Successfully executed 'config' script function");
            Ok(())
        }
        Err(e) => {
            error!("Runtime error while running 'config' function: {e}");
            Err(ScriptError::new(e.to_string()))
        }
    }
}

/// Renders compiler diagnostics as coloured text, falling back to `fallback`.
fn render_diagnostics(
    diagnostics: &rune::Diagnostics,
    sources: &rune::Sources,
    fallback: String,
) -> String {
    let mut buffer = termcolor::Buffer::ansi();
    if let Err(emit_err) = diagnostics.emit(&mut buffer, sources) {
        error!("Failed to emit compilation diagnostics to buffer: {emit_err}");
    }

    let rendered = String::from_utf8_lossy(&buffer.into_inner()).into_owned();
    if rendered.trim().is_empty() {
        fallback
    } else {
        rendered
    }
}
