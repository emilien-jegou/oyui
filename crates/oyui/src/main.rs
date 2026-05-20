use clap::Parser;
use core_lib::syntax::SyntaxEngine;
use std::fs;
use std::path::Path;
use std::time::Duration;
use std::{io, sync::Arc};

use core_lib::tree::FileTree;
use core_lib::worker::spawn_worker;
use crossbeam_channel::unbounded;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub mod app;
pub mod cli;
pub mod draw;
pub mod glob;
pub mod input;
pub mod ui_state;
pub mod view;

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

    let engine = Arc::new(SyntaxEngine::new());

    let (req_tx, req_rx) = unbounded();
    let (ev_tx, ev_rx) = unbounded();
    let worker_handle = spawn_worker(req_rx, ev_tx, engine);

    let mut app = App::new(req_tx.clone(), ev_rx);

    // Store base/output for merge scenarios
    app.base_path = opts.base.clone();
    app.output_path = opts.output.clone();

    // ── Build tree based on mode ─────────────────────────────────────────────
    if is_dir_empty(&opts.left) || is_dir_empty(&opts.right) {
        eprintln!("One of the target directories is empty. Aborting split.");
        std::process::exit(2);
    }

    let (tree, files_to_stat) = FileTree::build_from_dir_diff(&opts.left, &opts.right);

    if tree.nodes.is_empty() {
        eprintln!("No modifications found between directories. Nothing to split.");
        std::process::exit(2);
    }

    app.tree = tree;

    // Queue background diff stats for all discovered files
    for (rel_path, left, right) in files_to_stat {
        let _ = req_tx.send(core_lib::worker::WorkerRequest::ComputeStats {
            node_path: rel_path,
            left_path: left,
            right_path: right,
        });
    }

    // ── Terminal setup ───────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // ── Event loop ───────────────────────────────────────────────────────────
    let mut aborted = false;
    loop {
        app.tick();
        terminal.draw(|f| draw(f, &mut app))?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                // The `input::handle_key` function now centrally routes input
                // based on the App's command state and the View's current active sub-view.
                let exit_action = handle_key(&mut app, key)
                    .unwrap_or_else(|e| ExitAction::QuitWithReason(format!("Error: {e}")));

                match exit_action {
                    ExitAction::QuitAndMerge => {
                        break;
                    }
                    ExitAction::QuitWithReason(reason) => {
                        eprintln!("{}", reason);
                        aborted = true;
                        break;
                    }
                    ExitAction::QuitWithAbort => {
                        eprintln!("User Abort");
                        aborted = true;
                        break;
                    }
                    ExitAction::KeepRunning => {}
                }
            }
        }

        // If an inner system flagged a quit command (e.g., a successful merge write)
        if app.should_quit {
            break;
        }
    }

    // ── Cleanup ──────────────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let _ = app
        .worker_tx
        .send(core_lib::worker::WorkerRequest::Shutdown);
    let _ = worker_handle.await;

    if aborted {
        std::process::exit(1);
    }

    Ok(())
}
