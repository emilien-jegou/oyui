use clap::Parser;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::syntax::SyntaxEngine;
use crate::tree::FileTree;
use crate::worker::context::AppWorkerContext;
use crate::worker::{tasks, Tasker};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod cli;
pub mod commons;
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

use crate::app::{App, ExitAction};
use crate::cli::Opts;
use crate::draw::draw;
use crate::input::handle_key;

fn is_dir_empty(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut d| d.next().is_none())
        .unwrap_or(true)
}

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
    tracing::debug!(?opts, "Parsed CLI options");

    let worker = Tasker::spawn(
        AppWorkerContext::builder()
            .syntax_engine(SyntaxEngine::new())
            .config(opts.clone())
            .build(),
    );

    // 2. Pass the cloned worker wrapper into your App
    let mut app = App::new(worker);
    app.base_path = opts.base.clone();

    // ── Build tree based on mode ─────────────────────────────────────────────
    if is_dir_empty(&opts.left) || is_dir_empty(&opts.right) {
        tracing::error!("One of the target directories is empty. Aborting split.");
        drop(trace_guard);
        std::process::exit(2);
    }

    tracing::info!(left = %opts.left.display(), right = %opts.right.display(), "Building file tree...");
    let (tree, files_to_stat) = FileTree::build_from_dir_diff(&opts.left, &opts.right);

    if tree.nodes.is_empty() {
        tracing::error!("No modifications found between directories. Nothing to split.");
        drop(trace_guard);
        std::process::exit(2);
    }

    app.tree = tree;

    tracing::info!(
        count = files_to_stat.len(),
        "Queueing background diff stats"
    );

    let _ = app.worker.send(tasks::stats::StatsReq {
        files: files_to_stat,
    });

    tracing::debug!("Initializing terminal");
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    tracing::info!("Entering main event loop");
    let mut aborted = false;
    loop {
        app.tick().await;
        terminal.draw(|f| draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                tracing::trace!(?key, "Key event received");

                // The `input::handle_key` function now centrally routes input
                // based on the App's command state and the View's current active sub-view.
                let exit_action = handle_key(&mut app, key)
                    .unwrap_or_else(|e| ExitAction::QuitWithReason(format!("Error: {e}")));

                match exit_action {
                    ExitAction::QuitAndMerge => {
                        tracing::info!("Exiting event loop: QuitAndMerge");
                        break;
                    }
                    ExitAction::QuitWithReason(reason) => {
                        tracing::error!(%reason, "Exiting event loop: QuitWithReason");
                        aborted = true;
                        break;
                    }
                    ExitAction::QuitWithAbort => {
                        tracing::warn!("Exiting event loop: User Abort");
                        aborted = true;
                        break;
                    }
                    ExitAction::KeepRunning => {}
                }
            }
        }

        // If an inner system flagged a quit command (e.g., a successful merge write)
        if app.should_quit {
            tracing::info!("app.should_quit flag raised. Exiting event loop.");
            break;
        }
    }

    // ── Cleanup ──────────────────────────────────────────────────────────────
    tracing::debug!("Restoring terminal state");
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
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
