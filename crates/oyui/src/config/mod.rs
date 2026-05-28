use std::path::{Path, PathBuf};
use tracing::{debug, error, info, info_span, warn, Instrument};

pub mod builtin;
pub mod define_default_theme;
pub mod script;
pub mod theme;

pub use builtin::{fallback_theme, get_embedded_themes};
pub use define_default_theme::derive_ui_theme;
pub use theme::{LineHighlightMode, UiTheme};

// Keep ratatui out of build.rs deps — the From impl lives here.
impl From<theme::Color> for ratatui::style::Color {
    fn from(c: theme::Color) -> Self {
        match c {
            theme::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        }
    }
}

/// Load and execute a `.rn` config script, returning the resolved
/// [`UiTheme`] and the underlying `syntect` [`Theme`].
pub fn load_config_and_theme(
    path: &Path,
) -> Result<(UiTheme, syntect::highlighting::Theme), Box<dyn std::error::Error>> {
    let span = info_span!("load_config", path = %path.display());
    let _enter = span.enter();

    // ── Check if config is absent ───────────────────────────────────────────
    if !path.exists() {
        info!("Config file is absent. Using fallback theme.");
        let (ui, tm) = fallback_theme();
        return Ok((ui, tm));
    }

    info!("Config file found. Preparing to compile.");

    // Compile the script and build a VM.
    let mut vm = match script::build_vm(path) {
        Ok(vm) => vm,
        Err(e) => {
            error!("Failed to compile or build VM: {e}");
            return Err(e);
        }
    };

    // ── Phase 1: config(ctx) — choose the base theme ──────────────────────
    let cfg_ctx = match script::run_config_fn(&mut vm) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("Failed during execution of 'config' phase: {e}");
            return Err(e);
        }
    };

    let (mut ui_theme, tm_theme) = if let Some(tm_path) = &cfg_ctx.tm_theme_path {
        info!(custom_tm_theme = %tm_path, "Using custom .tmTheme path");
        let t = syntect::highlighting::ThemeSet::get_theme(tm_path)?;
        let ui = derive_ui_theme(&t);
        (ui, t)
    } else {
        let name = cfg_ctx
            .chosen_theme
            .as_deref()
            .unwrap_or("catppuccin-mocha");

        info!(resolved_theme = name, "Resolving base theme selection");
        if let Some((emb_ui, emb_tm)) = get_embedded_themes().get(name) {
            (emb_ui.clone(), emb_tm.clone())
        } else {
            warn!(
                theme_name = name,
                "Theme not found in embedded list. Using fallback."
            );
            let (ui, tm) = fallback_theme();
            (ui, tm)
        }
    };

    // ── Phase 2: theme(t) — apply per-field overrides ─────────────────────
    let mut vm2 = match script::build_vm(path) {
        Ok(vm) => vm,
        Err(e) => {
            error!("Failed to rebuild VM for theme customization: {e}");
            return Err(e);
        }
    };

    let theme_ctx = match script::run_theme_fn(&mut vm2, ui_theme) {
        Ok(ctx) => ctx,
        Err(e) => {
            error!("Failed during execution of 'theme' phase: {e}");
            return Err(e);
        }
    };
    ui_theme = theme_ctx.inner;

    info!("Config and theme loaded successfully");
    Ok((ui_theme, tm_theme))
}

/// Spawn a background task that re-executes the config script whenever the
pub fn watch_config(
    config_path: PathBuf,
    tx: tokio::sync::mpsc::Sender<Result<(UiTheme, syntect::highlighting::Theme), String>>,
) {
    let span = info_span!("watch_config", path = %config_path.display());

    tokio::spawn(
        async move {
            info!("Config file watcher started");
            let mut last_mtime = None;
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));

            loop {
                interval.tick().await;

                match std::fs::metadata(&config_path) {
                    Ok(meta) => {
                        if let Ok(mtime) = meta.modified() {
                            if Some(mtime) != last_mtime {
                                debug!(?mtime, ?last_mtime, "Config file modification detected");
                                last_mtime = Some(mtime);

                                let res =
                                    load_config_and_theme(&config_path).map_err(|e| e.to_string());

                                match res {
                                    Ok(theme) => {
                                        info!("Sending updated theme downstream");
                                        let _ = tx.send(Ok(theme)).await;
                                    }
                                    Err(err_msg) => {
                                        error!("Failed to reload config: {err_msg}");
                                        let _ = tx.send(Err(err_msg)).await;
                                    }
                                };
                            }
                        }
                    }
                    Err(e) => {
                        debug!(error = %e, "Could not query config metadata");
                    }
                }
            }
        }
        .instrument(span),
    );
}
