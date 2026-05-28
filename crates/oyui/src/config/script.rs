use std::sync::Arc;
use tracing::{debug, error, info, info_span, warn};

use rune::runtime::{BorrowRef, Protocol};
use rune::{termcolor, Any, ContextError, Module, Value, Vm};

use super::theme::{parse_hex_color, parse_highlight_mode, Color, UiTheme};

// ── ConfigCtx ────────────────────────────────────────────────────────────────

#[derive(Any, Debug, Default, Clone)]
pub struct ConfigCtx {
    pub chosen_theme: Option<String>,
    pub tm_theme_path: Option<String>,
}

impl ConfigCtx {
    #[rune::function(instance)]
    pub fn set_theme(&mut self, name: &str) {
        self.chosen_theme = Some(name.to_string());
        self.tm_theme_path = None;
    }

    #[rune::function(instance)]
    pub fn set_tm_theme(&mut self, path: &str) {
        self.tm_theme_path = Some(path.to_string());
        self.chosen_theme = None;
    }
}

// ── ThemeCtx ─────────────────────────────────────────────────────────────────

#[derive(Any, Debug)]
pub struct ThemeCtx {
    pub inner: UiTheme,
    pub is_dark: bool,
}

impl ThemeCtx {
    pub fn new(ui_theme: UiTheme) -> Self {
        let is_dark = match ui_theme.bg {
            Color::Rgb(r, g, b) => {
                let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
                lum < 128.0
            }
        };
        Self {
            inner: ui_theme,
            is_dark,
        }
    }

    #[rune::function(instance)]
    pub fn set_bg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.bg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_fg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.fg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_cursor_bg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.cursor_bg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_dim(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.dim = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_staged(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.staged = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_unstaged(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.unstaged = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_partial(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.partial = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_dir(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.dir = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_cmd(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.cmd = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_add_bg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.add_bg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_del_bg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.del_bg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_add_fg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.add_fg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_del_fg(&mut self, s: &str) {
        if let Some(c) = parse_hex_color(s) {
            self.inner.del_fg = c;
        }
    }
    #[rune::function(instance)]
    pub fn set_file_staged_highlight(&mut self, s: &str) {
        if let Some(m) = parse_highlight_mode(s, 0.08) {
            self.inner.file_staged_highlight = m;
        }
    }
    #[rune::function(instance)]
    pub fn set_file_staged_highlight_opacity(&mut self, v: f64) {
        self.inner.file_staged_highlight_opacity = v;
    }
    #[rune::function(instance)]
    pub fn set_file_change_highlight(&mut self, s: &str) {
        if let Some(m) = parse_highlight_mode(s, 0.20) {
            self.inner.file_change_highlight = m;
        }
    }
    #[rune::function(instance)]
    pub fn set_file_change_highlight_opacity(&mut self, v: f64) {
        self.inner.file_change_highlight_opacity = v;
    }
    #[rune::function(instance)]
    pub fn get_is_dark(&self) -> bool {
        self.is_dark
    }
}

// ── Module registration ───────────────────────────────────────────────────────

pub fn create_module() -> Result<Module, ContextError> {
    let mut m = Module::with_crate("app")?;

    m.ty::<ConfigCtx>()?;
    m.function_meta(ConfigCtx::set_theme)?;
    m.function_meta(ConfigCtx::set_tm_theme)?;

    m.ty::<ThemeCtx>()?;
    m.function_meta(ThemeCtx::set_bg)?;
    m.function_meta(ThemeCtx::set_fg)?;
    m.function_meta(ThemeCtx::set_cursor_bg)?;
    m.function_meta(ThemeCtx::set_dim)?;
    m.function_meta(ThemeCtx::set_staged)?;
    m.function_meta(ThemeCtx::set_unstaged)?;
    m.function_meta(ThemeCtx::set_partial)?;
    m.function_meta(ThemeCtx::set_dir)?;
    m.function_meta(ThemeCtx::set_cmd)?;
    m.function_meta(ThemeCtx::set_add_bg)?;
    m.function_meta(ThemeCtx::set_del_bg)?;
    m.function_meta(ThemeCtx::set_add_fg)?;
    m.function_meta(ThemeCtx::set_del_fg)?;
    m.function_meta(ThemeCtx::set_file_staged_highlight)?;
    m.function_meta(ThemeCtx::set_file_staged_highlight_opacity)?;
    m.function_meta(ThemeCtx::set_file_change_highlight)?;
    m.function_meta(ThemeCtx::set_file_change_highlight_opacity)?;
    m.function_meta(ThemeCtx::get_is_dark)?;

    Ok(m)
}

/// Compile and return a ready [`Vm`] from a `.rn` source file.
pub fn build_vm(path: &std::path::Path) -> Result<Vm, Box<dyn std::error::Error>> {
    let span = info_span!("build_vm", path = %path.display());
    let _enter = span.enter();

    debug!("Registering runtime context and modules");
    let mut context = rune::Context::with_default_modules()?;
    context.install(create_module()?)?;
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

/// Call the script's `config(ctx)` function and return the populated [`ConfigCtx`].
pub fn run_config_fn(vm: &mut Vm) -> Result<ConfigCtx, Box<dyn std::error::Error>> {
    let span = info_span!("run_config_fn");
    let _enter = span.enter();

    let value = rune::to_value(ConfigCtx::default())?;
    let arg = value.clone();

    match vm.call(["config"], (arg,)) {
        Ok(_) => {
            info!("Successfully executed 'config' script function");
            let ctx: BorrowRef<ConfigCtx> = value.borrow_ref()?;
            Ok(ctx.clone())
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("Missing function") || err_str.contains("Missing entry") {
                debug!("Function 'config' is absent in the script. Using default values.");
                let ctx: BorrowRef<ConfigCtx> = value.borrow_ref()?;
                Ok(ctx.clone())
            } else {
                error!("Runtime error while running 'config' function: {e}");
                Err(e.into())
            }
        }
    }
}

/// Call the script's `theme(t)` function and return the mutated [`ThemeCtx`].
pub fn run_theme_fn(
    vm: &mut Vm,
    ui_theme: UiTheme,
) -> Result<ThemeCtx, Box<dyn std::error::Error>> {
    let span = info_span!("run_theme_fn");
    let _enter = span.enter();

    let value = rune::to_value(ThemeCtx::new(ui_theme))?;
    let arg = value.clone();

    match vm.call(["theme"], (arg,)) {
        Ok(_) => {
            info!("Successfully executed 'theme' script function");
            let ctx = value.downcast::<ThemeCtx>()?;
            Ok(ctx)
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("Missing function") || err_str.contains("Missing entry") {
                debug!("Function 'theme' is absent in the script. Retaining original theme properties.");
                let ctx = value.downcast::<ThemeCtx>()?;
                Ok(ctx)
            } else {
                error!("Runtime error while running 'theme' function: {e}");
                Err(e.into())
            }
        }
    }
}
