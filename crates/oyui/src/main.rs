use clap::Parser;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::syntax::SyntaxEngine;
use crate::tree::FileTree;
use crate::worker::context::AppWorkerContext;
use crate::worker::{tasks, Tasker};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod cli;
pub mod commons;
pub mod config; // Add config module
pub mod diff;
pub mod diff_cache;
pub mod draw;
pub mod glob;
pub mod input;
pub mod syntax;
pub mod tree;
pub mod ui_state;
pub mod view;
pub mod worker;

pub use crate::view::ViewAction;
use crate::app::{App, ExitAction};
use crate::cli::Opts;
use crate::draw::draw;
use crate::input::handle_key;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Opts::parse();

    let trace_guard = commons::tracing::Tracer::builder()
        .flamegraph_enable(opts.flamegraph_enable)
        .flamegraph_save_file(opts.flamegraph_save_file.clone())
        .log_enable(opts.log_enable)
        .log_save_path(opts.log_save_path.clone())
        .log_console(opts.log_console)
        .build()
        .setup()?;

    tracing::info!("Starting oyui...");

    let config_path = opts.config.clone().unwrap_or(
        dirs::config_dir()
            .map(|d| d.join("oyui/config.toml"))
            .unwrap_or_else(|| PathBuf::from(".config/oyui/config.toml")),
    );

    let (ui_theme, syntax_theme) = config::load_config_and_theme(&config_path)
        .unwrap_or_else(|_| config::builtin::fallback_theme());

    // Spawn async background config watcher (returns channel of tuples)
    let (theme_tx, mut theme_rx) = tokio::sync::mpsc::channel::<(
        crate::config::theme::UiTheme,
        syntect::highlighting::Theme,
    )>(1);
    crate::config::watch_config(config_path, theme_tx);

    let worker = Tasker::spawn(
        AppWorkerContext::builder()
            .syntax_engine(SyntaxEngine::new())
            .algorithm(opts.diff_algorithm)
            .config(opts.clone())
            .build(),
    );

    let mut app = App::new(worker);
    app.theme = ui_theme;
    app.syntax_theme = std::sync::Arc::new(syntax_theme);
    app.base_path = opts.base.clone();
    app.left_path = Some(opts.left.clone());
    app.right_path = Some(opts.right.clone());
    app.view.file_view.scrolloff = opts.scrolloff;
    app.view.file_view.context_lines = opts.context_lines;
    app.view.tree_view.scrolloff = opts.scrolloff;

    tracing::info!(left = %opts.left.display(), right = %opts.right.display(), "Building file tree...");
    let (tree, files_to_stat) = FileTree::build_from_dir_diff(&opts.left, &opts.right);

    if tree.nodes.is_empty() {
        tracing::error!("No modifications found between directories. Nothing to split.");
        drop(trace_guard);
        std::process::exit(2);
    }

    app.tree = tree;

    let _ = app.worker.send(tasks::stats::StatsReq {
        files: files_to_stat,
    });

    tracing::debug!("Initializing terminal");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    tracing::info!("Entering main event loop");
    let mut aborted = false;
    loop {
        if let Ok((new_ui_theme, new_syntax_theme)) = theme_rx.try_recv() {
            tracing::info!("Config file change detected. Live reloading theme.");
            app.theme = new_ui_theme;
            app.syntax_theme = std::sync::Arc::new(new_syntax_theme);
            app.cache.syntax.clear();
        }

        app.tick().await;
        terminal.draw(|f| draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                let exit_action = handle_key(&mut app, key)
                    .unwrap_or_else(|e| ExitAction::QuitWithReason(format!("Error: {e}")));

                match exit_action {
                    ExitAction::QuitAndMerge => break,
                    ExitAction::QuitWithReason(reason) => {
                        tracing::error!(%reason, "Exiting event loop: QuitWithReason");
                        aborted = true;
                        break;
                    }
                    ExitAction::QuitWithAbort => {
                        aborted = true;
                        break;
                    }
                    ExitAction::KeepRunning => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    tracing::debug!("Restoring terminal state");
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    tracing::info!("Shutting down background worker...");
    let _ = app.shutdown().await;

    if aborted {
        tracing::warn!("Application aborted.");
        drop(trace_guard);
        std::process::exit(1);
    }

    tracing::info!("oyui shutting down cleanly.");
    Ok(())
}
