//! Main event loop: terminal setup, the input thread, dispatch, and teardown.

use super::{draw, App};
use crate::commands::CommandError;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

impl App {
    pub async fn run(&mut self) -> Result<(), CommandError> {
        tracing::debug!("Initializing terminal");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        tracing::info!("Entering main event loop");
        let mut aborted = false;

        // crossterm's blocking read runs on its own thread so the runtime
        // worker is never parked between frames. Each event carries the
        // instant it was read so logs can show key-to-paint latency.
        let (input_tx, mut input_rx) =
            tokio::sync::mpsc::unbounded_channel::<(std::time::Instant, Event)>();
        std::thread::spawn(move || loop {
            match event::read() {
                Ok(ev) => {
                    if input_tx.send((std::time::Instant::now(), ev)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(?e, "terminal input reader failed");
                    break;
                }
            }
        });

        let worker = self.worker.clone();
        terminal.draw(|f| draw::draw(f, self))?;

        loop {
            let mut branch = "sleep";
            let mut key_read_at: Option<std::time::Instant> = None;

            tokio::select! {
                maybe_input = input_rx.recv() => {
                    branch = "input";
                    // Drain every key queued during the previous frame: one
                    // redraw must cover the burst or latency compounds.
                    let mut next = maybe_input;
                    let mut first_read: Option<std::time::Instant> = None;
                    loop {
                        match next.take() {
                            Some((read_at, Event::Key(key))) => {
                                first_read.get_or_insert(read_at);
                                aborted = self.handle_key(key);
                                if aborted {
                                    break;
                                }
                            }
                            // Resize/mouse/focus events only need a redraw.
                            Some(_) => {}
                            None => {
                                return Err(CommandError::Runtime(
                                    "terminal input reader stopped".into(),
                                ));
                            }
                        }
                        match input_rx.try_recv() {
                            Ok(item) => next = Some(item),
                            // Empty: batch done. Closed: next recv() exits.
                            Err(_) => break,
                        }
                    }
                    key_read_at = first_read;
                }
                maybe_event = worker.recv() => {
                    branch = "worker";
                    match maybe_event {
                        Some(ev) => self.handle_worker_event(ev),
                        None => {
                            return Err(CommandError::Runtime(
                                "worker dispatcher stopped".into(),
                            ));
                        }
                    }
                    // Drain the rest of the burst so one redraw covers it.
                    self.tick();
                }
                // Listener tasks finish without notifying the app; repaint
                // periodically to pick their results up.
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }

            if aborted || self.ui.lock().should_quit {
                if let Err(e) = self.config.call_event("quit") {
                    tracing::error!("quit event failed: {e}");
                }
                break;
            }
            let draw_started = std::time::Instant::now();
            terminal.draw(|f| draw::draw(f, self))?;
            // key_ms: key read -> painted (handle + draw). -1 when no key.
            tracing::debug!(
                branch,
                key_ms = key_read_at.map_or(-1.0, |t| t.elapsed().as_secs_f64() * 1000.0),
                draw_ms = draw_started.elapsed().as_secs_f64() * 1000.0,
                "event loop iteration"
            );
        }

        tracing::debug!("Restoring terminal state");
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        tracing::info!("Shutting down background worker...");
        let _ = self.shutdown().await;

        if aborted {
            tracing::warn!("Application aborted.");
            return Err(CommandError::Aborted);
        }

        Ok(())
    }
}
