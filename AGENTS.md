# AGENTS.md

## Terminal rendering — imperative, applies to any change on the tty path

> If you mutate the backend directly: Direct backend changes bypass Ratatui's
> diffing and viewport bookkeeping. After doing that, run a full draw pass or
> clear the terminal before assuming Ratatui's internal view still matches the
> screen.

How oyui applies that rule. The canonical implementation is
`crates/oyui/src/app/run.rs`; read it before touching any render, input, or
teardown code.

1. **Ratatui is open-loop.** `Terminal` keeps its `previous` buffer in sync
   with what it last *sent*, never with what the screen shows. A delta that is
   lost or garbled after `Buffer::diff` computed it is never retried: the next
   frame diffs against the already-advanced buffer, the affected cells compare
   equal, and the screen stays stale until both buffers are reset. Only a
   reset — a resize, or oyui's `force_full_repaint` — repairs it.

2. **One writer for the tty.** Every byte to the terminal goes through
   `terminal.backend_mut()` (the `CrosstermBackend<CountingStdout>` owned by
   the event loop). Handlers must never write `io::stdout()` directly:
   out-of-band sequences (OSC 52 clipboard) are pushed to
   `SessionContext::terminal_writes` and the event loop emits them through
   the backend *between frames*. A direct write can interleave with a frame's
   diff bytes and cut it apart — the confirmed trigger of the stale-highlight
   bug this file exists to prevent.

3. **Make presentation reconcile instead of assuming the delta landed.** Each
   frame is preceded by a style-only snapshot (`PaintKey` in `app/run.rs`);
   when it changed since the previous frame — or every
   `FULL_REPAINT_INTERVAL` (2 s safety net) — the frame is written in full via
   `force_full_repaint`: two public `swap_buffers()` calls, which empties both
   buffers. That is the same reset a resize performs, using public API only.

4. **Never `Terminal::clear()`** as a resync primitive: it writes `\x1b[6n`
   and reads stdin, stalling the event loop (~2 s observed in-session) and
   able to swallow a keypress. Do not reach for `Terminal::resize` either: it
   flushes `\x1b[2J` and visibly flashes. Both are replaced by
   `force_full_repaint`.

5. **Lock ordering:** when a code path holds more than one session lock, take
   `app.theme` before `app.ui` — never the reverse (see `paint_key`).

## Working agreements

- `crate::commons::frame_log` and `OYUI_INPUT_PROBE` are diagnostic
  instrumentation; keep them compilable and do not let them write the tty
  themselves (the frame log appends to a file sibling of the log).
- `crates/syndiff/**` is user work-in-progress: do not reformat, move, or
  "clean up" anything there.
