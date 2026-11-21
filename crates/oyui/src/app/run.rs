//! Main event loop: terminal setup, the input thread, dispatch, and teardown.

use super::{draw, App};
use crate::commands::CommandError;
use crate::commons::frame_log;
use crate::commons::frame_log::epoch_millis;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_lite::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Write};
use std::time::Duration;

type Tui = Terminal<CrosstermBackend<crate::commons::frame_log::CountingStdout>>;

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
        let backend = CrosstermBackend::new(crate::commons::frame_log::CountingStdout::new());
        let mut terminal = Terminal::new(backend)?;
        // Armed from here on: both `?` returns and panics unwinding out of
        // the loop pass through the guard's restore.
        let mut restore = TerminalRestore {
            terminal: &mut terminal,
            armed: true,
        };

        tracing::info!("Entering main event loop");
        let mut aborted = false;

        // The input is a direct branch of this loop's `select!` — no
        // relaying task and no mpsc hop. The instrumented run showed keys
        // already queued in the channel while the loop stayed parked up to
        // ~1s: every intermediate hop removed there is a wakeup that can no
        // longer starve. Crossterm's `EventStream::drop` signals its worker
        // (`stream_wake_task_should_shutdown` + waker), which is the
        // bounded-teardown property the stopped reader provided (6.19).
        let mut events = EventStream::new();

        let worker = self.worker.clone();
        restore.terminal.draw(|f| draw::draw(f, self))?;

        loop {
            let branch;

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
                maybe_event = events.next() => {
                    branch = "input";
                    // One event per iteration, then one draw: noop-waker
                    // draining is what broke the session — EventStream's
                    // pending-poll sends its worker a Task carrying the
                    // caller's waker, so a drain polled with Waker::noop()
                    // registered an unreachable wakeup and starved the loop
                    // (the real poll's compare_exchange then saw
                    // `executed == true` and never re-registered the loop's
                    // real waker). Per-event frames cost 9–13 ms; the batch
                    // optimization is not worth a parked-forever mode.
                    match maybe_event {
                        // Resize/mouse/focus go straight to the redraw.
                        Some(Ok(Event::Key(key))) => {
                            frame_log::frame(&[
                                ("src", "key".to_string()),
                                ("ms", epoch_millis().to_string()),
                            ]);
                            aborted = self.handle_key(key);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::error!(?e, "terminal input stream failed");
                            return Err(CommandError::Runtime(
                                format!("terminal input stream failed: {e}").into(),
                            ));
                        }
                        None => {
                            return Err(CommandError::Runtime(
                                "terminal input stream ended".into(),
                            ));
                        }
                    }
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
            // pending stays near zero: coalescing is what keeps a burst of
            // per-file results from queueing a frame each.
            tracing::debug!(
                branch,
                draw_ms = draw_started.elapsed().as_secs_f64() * 1000.0,
                pending = worker.pending(),
                "event loop iteration"
            );
            // Per-iteration frame line: the scroll states the next frame will
            // read, linearized so the log needs no serde. One line per entry.
            let current_path = self.ui.lock().file_view.current_path.clone();
            if let Some(path) = current_path {
                if let Some(st) = self
                    .ui
                    .lock()
                    .file_view
                    .scroll_states
                    .get(&path)
                {
                    frame_log::frame(&[
                        ("src", "scroll".to_string()),
                        ("ms", epoch_millis().to_string()),
                        ("selected", st.selected().unwrap_or(0).to_string()),
                        ("offset", st.offset().to_string()),
                    ]);
                }
            }
        }

        tracing::debug!("Restoring terminal state");
        // The `EventStream` drops with this scope: its `Drop` signals the
        // worker thread and wakes its blocking poll, so no fd read can
        // outlive raw mode (finding 6.19's bounded teardown, no join).
        // Disarm so the explicit restore reports errors, instead of the Drop
        // already having consumed the terminal state.
        drop(events);
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
