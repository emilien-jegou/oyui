//! Main event loop: terminal setup, the input thread, dispatch, and teardown.

use super::{draw, App};
use crate::commands::CommandError;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;

type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Restores the terminal when dropped, including during panic unwinding.
///
/// Every early return in the event loop (`?`, a panic mid-frame) passes
/// through this guard, so a crash can no longer leave the user's shell in
/// raw mode on the alternate screen. The normal teardown path disarms the
/// guard and restores explicitly so failures are still reported.
struct TerminalRestore<'a> {
    terminal: &'a mut Tui,
    armed: bool,
}

impl TerminalRestore<'_> {
    fn disarm(&mut self) {
        self.armed = false;
    }

    /// Best-effort restore; errors are logged, never propagated.
    fn restore(&mut self) {
        if let Err(e) = crossterm::terminal::disable_raw_mode() {
            tracing::error!("failed to disable raw mode during restore: {e}");
        }
        if let Err(e) = crossterm::execute!(self.terminal.backend_mut(), LeaveAlternateScreen) {
            tracing::error!("failed to leave alternate screen during restore: {e}");
        }
        if let Err(e) = self.terminal.show_cursor() {
            tracing::error!("failed to show cursor during restore: {e}");
        }
        let _ = io::stdout().flush();
    }
}

impl Drop for TerminalRestore<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.restore();
        }
    }
}

impl App {
    pub async fn run(&mut self) -> Result<(), CommandError> {
        tracing::debug!("Initializing terminal");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        // Armed from here on: both `?` returns and panics unwinding out of
        // the loop pass through the guard's restore.
        let mut restore = TerminalRestore {
            terminal: &mut terminal,
            armed: true,
        };

        tracing::info!("Entering main event loop");
        let mut aborted = false;

        // crossterm's blocking read runs on its own thread so the runtime
        // worker is never parked between frames. Each event carries the
        // instant it was read so logs can show key-to-paint latency.
        //
        // The reader polls on a tick instead of blocking forever so teardown
        // can stop and join it before the terminal state is torn down
        // (finding 6.19): `poll` wakes the instant an event arrives, so the
        // tick only bounds how long a stop request can go unnoticed — never
        // key latency.
        let (input_tx, mut input_rx) =
            tokio::sync::mpsc::unbounded_channel::<(std::time::Instant, Event)>();
        let reader_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let stop = reader_stop.clone();
            std::thread::spawn(move || loop {
                match event::poll(Duration::from_millis(250)) {
                    Ok(true) => match event::read() {
                        Ok(ev) => {
                            if input_tx.send((std::time::Instant::now(), ev)).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::error!(?e, "terminal input reader failed");
                            break;
                        }
                    },
                    Ok(false) => {}
                    Err(e) => {
                        // Poll fails once stdin is gone; stop reading.
                        tracing::error!(?e, "terminal input poll failed");
                        break;
                    }
                }
                if stop.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
            })
        };

        let worker = self.worker.clone();
        restore.terminal.draw(|f| draw::draw(f, self))?;

        loop {
            let mut branch = "idle";
            let mut key_read_at: Option<std::time::Instant> = None;

            // A frame used to be repainted 20x/second so results produced by
            // listeners that announced nothing could still be picked up. The
            // worker now signals every state change (tree, stats, diff,
            // syntax, config, theme) and internal plumbing never wakes the app,
            // so the only time-based wakeup left is a transient message expiry.
            // With no message on screen the loop sleeps until something real
            // happens.
            let message_expires = self.ui.lock().message.as_ref().map(|m| m.expires_at);
            let message_deadline = message_expires
                .unwrap_or_else(|| std::time::Instant::now() + Duration::from_secs(3600));
            let message_timer = tokio::time::sleep_until(message_deadline.into());

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
                // Only fires while a transient message is on screen.
                _ = message_timer => { branch = "message"; }
            }

            if aborted || self.ui.lock().should_quit {
                if let Err(e) = self.config.call_event("quit") {
                    tracing::error!("quit event failed: {e}");
                }
                break;
            }

            // Resolve script callbacks waiting on an off-thread answer before
            // painting, so the frame sees the result they produce.
            self.drain_pending_scripts();

            let draw_started = std::time::Instant::now();
            restore.terminal.draw(|f| draw::draw(f, self))?;
            // key_ms: key read -> painted (handle + draw). -1 when no key.
            // pending stays near zero: coalescing is what keeps a burst of
            // per-file results from queueing a frame each.
            tracing::debug!(
                branch,
                key_ms = key_read_at.map_or(-1.0, |t| t.elapsed().as_secs_f64() * 1000.0),
                draw_ms = draw_started.elapsed().as_secs_f64() * 1000.0,
                pending = worker.pending(),
                "event loop iteration"
            );
        }

        tracing::debug!("Restoring terminal state");
        // Stop and join the input reader before tearing the terminal down,
        // so no blocked read(2) can outlive raw mode (finding 6.19): the
        // poll tick bounds the joint wait to one interval.
        reader_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = reader.join();
        // Disarm so the explicit restore reports errors, instead of the Drop
        // already having consumed the terminal state.
        restore.disarm();
        disable_raw_mode()?;
        execute!(restore.terminal.backend_mut(), LeaveAlternateScreen)?;
        restore.terminal.show_cursor()?;

        tracing::info!("Shutting down background worker...");
        let _ = self.shutdown().await;

        if aborted {
            tracing::warn!("Application aborted.");
            return Err(CommandError::Aborted);
        }
        if self.no_modifications {
            return Err(CommandError::NoModifications);
        }

        Ok(())
    }
}
