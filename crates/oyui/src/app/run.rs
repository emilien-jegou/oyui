//! Main event loop: terminal setup, the input thread, dispatch, and teardown.
//!
//! Two architectural rules live here (root `AGENTS.md`, "Terminal rendering"):
//! the backend is the only tty writer, and presentation reconciles instead of
//! assuming a diff landed (`PaintKey` + `force_full_repaint` below).

use super::{draw, App};
use crate::commands::CommandError;
use crate::commons::frame_log;
use crate::commons::frame_log::epoch_millis;
use crate::config::UiTheme;
use crate::view::ViewKind;
use crate::worker::events::file_opened::FileOpened;
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_lite::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::time::Duration;
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Instant,
};

type Tui = Terminal<CrosstermBackend<crate::commons::frame_log::CountingWriter>>;

/// Longest a frame may go without being written in full.
///
/// Ratatui keeps no record of what the terminal actually shows, only of what it
/// last sent, so a delta that is lost anywhere — a direct backend write, a
/// partially applied frame, a terminal that coalesces output — is never
/// retried. The interval bounds how long such a desync can survive; the
/// per-frame check below catches the case that actually bit us.
const FULL_REPAINT_INTERVAL: Duration = Duration::from_secs(2);

/// State whose change only restyles cells and never changes a glyph.
///
/// Ratatui diffs the frame it is about to draw against the frame it last
/// *sent*, not against the screen. A dropped delta is therefore permanent: the
/// cells already compare equal, so the stale style is never re-emitted, and
/// only a resize — the one thing that resets both buffers — repairs it. Moving
/// the cursor, switching panes, folding and changing the theme are exactly the
/// style-only transitions, so the frame that follows one is written in full.
#[derive(Clone, PartialEq)]
struct PaintKey {
    view: ViewKind,
    theme: UiTheme,
    tree_selected: usize,
    tree_offset: usize,
    path: Option<std::path::PathBuf>,
    file_selected: usize,
    file_offset: usize,
    hscroll: usize,
    folded: bool,
}

/// Snapshots the style-only part of the UI.
fn paint_key(app: &App) -> PaintKey {
    // Theme first: never hold the UI lock while taking another lock.
    let theme = app.theme.read().ui.clone();
    let ui = app.ui.lock();
    let file = &ui.file_view;
    let state = file
        .current_path
        .as_ref()
        .and_then(|path| file.scroll_states.get(path));
    PaintKey {
        view: ui.current,
        theme,
        tree_selected: ui.tree_view.selected_index,
        tree_offset: ui.tree_view.list_state.offset(),
        path: file.current_path.clone(),
        file_selected: state.and_then(|s| s.selected()).unwrap_or(0),
        file_offset: state.map(|s| s.offset()).unwrap_or(0),
        hscroll: file
            .current_path
            .as_ref()
            .and_then(|path| file.hscroll_states.get(path).copied())
            .unwrap_or(0),
        folded: file.is_folded,
    }
}

/// Whether a state change invalidates enough of the presentation to require a
/// full reconciliation. Cursor selection and scrolling are intentionally
/// excluded: Ratatui's normal buffer diff must emit those changed cells.
fn requires_full_repaint(previous: &PaintKey, current: &PaintKey) -> bool {
    previous.view != current.view
        || previous.theme != current.theme
        || previous.path != current.path
        || previous.hscroll != current.hscroll
        || previous.folded != current.folded
}

/// Discards both buffers so the next frame is written in full.
///
/// `Terminal` only knows what it last *sent*, never what the screen shows, so a
/// delta that is lost — a direct backend write, a partially applied frame, a
/// terminal that coalesces output — is never retried: the cells already compare
/// equal, so the stale style is never re-emitted, and only a resize (which
/// resets both buffers) repairs it.
///
/// Swapping twice empties the inactive buffer *and* the one about to be drawn
/// into, so the next `draw` diffs against nothing and re-emits every cell. It
/// uses only public API and is the same reset a resize performs, without the
/// cursor query of `Terminal::clear` (which writes `\x1b[6n` and reads stdin,
/// able to swallow a keypress) or the screen clear of `Terminal::resize`.
fn force_full_repaint(terminal: &mut Tui) {
    terminal.swap_buffers();
    terminal.swap_buffers();
}

/// Emits the out-of-band terminal sequences queued by handlers (OSC 52).
///
/// Handlers never touch the tty; they push to
/// `SessionContext::terminal_writes` and this is the sole drain, written
/// through the backend after a frame's flush. That keeps two properties:
///
/// - **One writer** — nothing interleaves with the bytes ratatui emits for a
///   frame, so a frame can never be cut mid-stream (the stale-highlight
///   bug's confirmed trigger; root `AGENTS.md`).
/// - **The reconcile pass stays sufficient** — out-of-band bytes land
///   between frames, so the paint-key full repaints only ever have to cover
///   bookkeeping done inside ratatui itself.
fn emit_terminal_writes(app: &App, terminal: &mut Tui) {
    let mut outbox = app.terminal_writes.lock();
    if outbox.is_empty() {
        return;
    }
    let backend = terminal.backend_mut();
    for sequence in outbox.drain(..) {
        if let Err(e) = backend.write_all(sequence.as_bytes()) {
            tracing::error!("failed to write out-of-band terminal sequence: {e}");
        }
    }
    if let Err(e) = backend.flush() {
        tracing::error!("failed to flush out-of-band terminal sequence: {e}");
    }
}

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
        let _ = self.terminal.backend_mut().flush();
    }
}

impl Drop for TerminalRestore<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.restore();
        }
    }
}

struct ProbedEvent {
    seq: u64,
    event: Event,
    read_at: Instant,
    read_epoch_ns: u128,
}

enum InputMessage {
    Event(Event, Option<ProbedEvent>),
}

enum InputEvents {
    Stream(EventStream),
    Probe {
        receiver: tokio::sync::mpsc::UnboundedReceiver<io::Result<ProbedEvent>>,
        shutdown: Arc<AtomicBool>,
        reader: Option<thread::JoinHandle<()>>,
    },
}

impl InputEvents {
    fn new(probe: bool) -> Self {
        if !probe {
            return Self::Stream(EventStream::new());
        }

        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();
        let reader = thread::spawn(move || {
            let mut seq = 0_u64;
            while !thread_shutdown.load(Ordering::Acquire) {
                match crossterm::event::poll(Duration::from_millis(10)) {
                    Ok(false) => continue,
                    Ok(true) => match crossterm::event::read() {
                        Ok(event) => {
                            seq += 1;
                            let read_at = Instant::now();
                            let read_epoch_ns = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            crate::commons::frame_log::frame(&[
                                ("src", "input_read".to_string()),
                                ("seq", seq.to_string()),
                                ("epoch_ns", read_epoch_ns.to_string()),
                                ("event", format!("{event:?}")),
                            ]);
                            if sender
                                .send(Ok(ProbedEvent {
                                    seq,
                                    event,
                                    read_at,
                                    read_epoch_ns,
                                }))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(error) => {
                            let _ = sender.send(Err(error));
                            break;
                        }
                    },
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });

        Self::Probe {
            receiver,
            shutdown,
            reader: Some(reader),
        }
    }

    async fn next(&mut self) -> Option<io::Result<InputMessage>> {
        match self {
            Self::Stream(events) => events
                .next()
                .await
                .map(|result| result.map(|event| InputMessage::Event(event, None))),
            Self::Probe { receiver, .. } => receiver.recv().await.map(|result| {
                result.map(|probed| InputMessage::Event(probed.event.clone(), Some(probed)))
            }),
        }
    }
}

impl Drop for InputEvents {
    fn drop(&mut self) {
        if let Self::Probe {
            shutdown, reader, ..
        } = self
        {
            shutdown.store(true, Ordering::Release);
            if let Some(reader) = reader.take() {
                let _ = reader.join();
            }
        }
    }
}

impl App {
    pub async fn run(&mut self) -> Result<(), CommandError> {
        tracing::debug!("Initializing terminal");
        let terminal_writer = frame_log::CountingWriter::terminal()?;
        enable_raw_mode()?;
        // The backend owns the tty from here on. Setup sequences are also
        // written through it — entering the alternate screen on a separate
        // handle is what allowed two writers to interleave on one terminal.
        let backend = CrosstermBackend::new(terminal_writer);
        let mut terminal = Terminal::new(backend)?;
        execute!(terminal.backend_mut(), EnterAlternateScreen)?;
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
        let input_probe = std::env::var_os("OYUI_INPUT_PROBE").is_some();
        let mut events = InputEvents::new(input_probe);

        let worker = self.worker.clone();
        let mut draw_id = 0_u64;
        let mut last_full_repaint = Instant::now();
        let mut last_rendered_buffer = None;
        restore.terminal.draw(|f| {
            draw::draw(f, self);
            last_rendered_buffer = Some(f.buffer_mut().clone());
        })?;
        let mut last_paint_key = Some(paint_key(self));

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

            // While a view waits on a missing diff, keep a slow tick alive:
            // the worker may have lost the claim (a cache invalidation
            // mid-flight), and a view must not hold `Loading...`
            // indefinitely — the diff itself is sub-100ms (measured), so a
            // 1s retry is silence-tolerant and heals every loss class.
            let requeue_path = {
                let ui = self.ui.lock();
                (ui.current == ViewKind::File)
                    .then(|| ui.file_view.current_path.clone())
                    .flatten()
            };
            let requeue_path = requeue_path.filter(|p| self.cache.diffs.get(p).is_none());
            let requeue_deadline = if requeue_path.is_some() {
                std::time::Instant::now() + Duration::from_secs(1)
            } else {
                std::time::Instant::now() + Duration::from_secs(3600)
            };
            let requeue_timer = tokio::time::sleep_until(requeue_deadline.into());

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
                        Some(Ok(InputMessage::Event(Event::Key(key), probed))) => {
                            let dispatch_ns = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            let read_to_dispatch_us =
                                probed.as_ref().map(|event| event.read_at.elapsed().as_micros());
                            tracing::debug!(
                                ?key,
                                probe_seq = probed.as_ref().map(|event| event.seq),
                                read_to_dispatch_us = ?read_to_dispatch_us,
                                "received key event"
                            );
                            let mut fields = vec![
                                ("src", "key".to_string()),
                                ("ms", epoch_millis().to_string()),
                                ("dispatch_epoch_ns", dispatch_ns.to_string()),
                            ];
                            if let Some(event) = &probed {
                                fields.push(("seq", event.seq.to_string()));
                                fields.push(("read_epoch_ns", event.read_epoch_ns.to_string()));
                                fields.push((
                                    "read_to_dispatch_us",
                                    event.read_at.elapsed().as_micros().to_string(),
                                ));
                            }
                            frame_log::frame(&fields);
                            aborted = self.handle_key(key);
                        }
                        Some(Ok(InputMessage::Event(_, _))) => {}
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
                // Missing-diff retry for the on-screen file: re-request the
                // diff at most once per second while it is missing. The
                // re-request is a no-op while a worker holds the claim, and
                // heals every loss class (a result lost to an invalidation
                // while its task was in flight) without user interaction.
                _ = requeue_timer => {
                    branch = "requeue";
                    if let Some(path) = &requeue_path {
                        if self.cache.diffs.get(path).is_none() {
                            let last = self.last_diff_request.get(path).copied();
                            if last.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
                                self.last_diff_request
                                    .insert(path.clone(), std::time::Instant::now());
                                let _ = self.worker.send(FileOpened { path: path.clone() });
                            }
                        }
                    }
                }
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
            draw_id += 1;
            frame_log::frame(&[
                ("src", "draw_begin".to_string()),
                ("draw_id", draw_id.to_string()),
                ("branch", branch.to_string()),
                ("ms", epoch_millis().to_string()),
            ]);
            let key = paint_key(self);
            let broad_change = last_paint_key
                .as_ref()
                .is_some_and(|previous| requires_full_repaint(previous, &key));
            let full_repaint = broad_change || last_full_repaint.elapsed() >= FULL_REPAINT_INTERVAL;
            if full_repaint {
                last_full_repaint = Instant::now();
                frame_log::frame(&[("src", "full_repaint".to_string())]);
                force_full_repaint(restore.terminal);
            }
            let mut rendered_buffer = None;
            restore.terminal.draw(|f| {
                draw::draw(f, self);
                rendered_buffer = Some(f.buffer_mut().clone());
            })?;
            last_paint_key = Some(paint_key(self));
            // Out-of-band sequences go after the frame: below the frame's
            // flush, through the same backend, never inside it.
            emit_terminal_writes(self, restore.terminal);
            let (view, writable, tree_selected, tree_offset, file_selected, file_offset) = {
                let ui = self.ui.lock();
                let file = &ui.file_view;
                let file_state = file
                    .current_path
                    .as_ref()
                    .and_then(|path| file.scroll_states.get(path));
                (
                    format!("{:?}", ui.current),
                    ui.writable,
                    ui.tree_view.selected_index,
                    ui.tree_view.list_state.offset(),
                    file_state.and_then(|state| state.selected()).unwrap_or(0),
                    file_state.map(|state| state.offset()).unwrap_or(0),
                )
            };
            frame_log::frame(&[
                ("src", "ui_state".to_string()),
                ("draw_id", draw_id.to_string()),
                ("view", view),
                ("writable", writable.to_string()),
                ("tree_selected", tree_selected.to_string()),
                ("tree_offset", tree_offset.to_string()),
                ("file_selected", file_selected.to_string()),
                ("file_offset", file_offset.to_string()),
            ]);
            if branch == "input" {
                if let Some(current) = rendered_buffer.as_ref() {
                    let reset_diff = full_repaint
                        || last_rendered_buffer
                            .as_ref()
                            .is_none_or(|previous| previous.area != current.area);
                    let changes = if reset_diff {
                        current
                            .area
                            .positions()
                            .map(|pos| (pos.x, pos.y, &current[(pos.x, pos.y)]))
                            .collect::<Vec<_>>()
                    } else {
                        last_rendered_buffer
                            .as_ref()
                            .map(|previous| previous.diff(current))
                            .unwrap_or_default()
                    };
                    let cells = changes
                        .iter()
                        .map(|(x, y, cell)| {
                            format!(
                                "({x},{y}) sym={:?} fg={:?} bg={:?} mod={:?}",
                                cell.symbol(),
                                cell.fg,
                                cell.bg,
                                cell.modifier
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(";");
                    frame_log::frame(&[
                        ("src", "rendered_cell_diff".to_string()),
                        ("draw_id", draw_id.to_string()),
                        ("reset_diff", reset_diff.to_string()),
                        ("cells", cells),
                    ]);
                }
            }
            last_rendered_buffer = rendered_buffer;
            frame_log::frame(&[
                ("src", "draw_end".to_string()),
                ("draw_id", draw_id.to_string()),
                ("ms", epoch_millis().to_string()),
            ]);
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
                if let Some(st) = self.ui.lock().file_view.scroll_states.get(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::{Color, Style},
    };

    fn key() -> PaintKey {
        PaintKey {
            view: ViewKind::File,
            theme: crate::theme::ThemeState::new(
                &crate::terminal_colors::TerminalColorMode::NoColor,
            )
            .ui,
            tree_selected: 0,
            tree_offset: 0,
            path: Some("file".into()),
            file_selected: 0,
            file_offset: 0,
            hscroll: 0,
            folded: false,
        }
    }

    #[test]
    fn cursor_and_scroll_changes_use_the_normal_diff() {
        let previous = key();
        let mut current = previous.clone();
        current.file_selected += 1;
        current.file_offset += 1;
        assert!(!requires_full_repaint(&previous, &current));
        current.tree_selected += 1;
        current.tree_offset += 1;
        assert!(!requires_full_repaint(&previous, &current));

        let area = Rect::new(0, 0, 2, 2);
        let previous_buffer = Buffer::empty(area);
        let mut current_buffer = previous_buffer.clone();
        current_buffer[(0, 0)].set_style(Style::default().bg(Color::Red));
        current_buffer[(0, 1)].set_style(Style::default().bg(Color::Blue));
        let changed = previous_buffer.diff(&current_buffer);
        assert_eq!(
            changed.iter().map(|(x, y, _)| (*x, *y)).collect::<Vec<_>>(),
            [(0, 0), (0, 1)]
        );
    }

    #[test]
    fn broad_visual_changes_still_force_reconciliation() {
        let previous = key();
        let mut current = previous.clone();
        current.theme.cursor_bg = crate::config::theme::Color::Rgb(1, 2, 3);
        assert!(requires_full_repaint(&previous, &current));

        current = previous.clone();
        current.path = Some("other-file".into());
        assert!(requires_full_repaint(&previous, &current));
    }
}
