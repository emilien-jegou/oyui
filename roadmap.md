# Roadmap: interactive terminal output under `jj diff`

## Goal

Make oyui's interactive UI render to the controlling terminal when Jujutsu
launches it as an external diff generator with stdout connected to a pipe. Keep
Jujutsu's diff-tool protocol on stdout, retain Ratatui as the only UI writer,
and leave direct-terminal sessions such as `jj split` unchanged.

This is a transport change to test the leading hypothesis, not a confirmed fix
for stale highlighting. The captured frames show the expected cursor state and
ANSI changes; the relay has not yet been shown to lose or delay them.

## Constraints

- All UI bytes must continue to pass through the event loop's Ratatui backend.
  No handler or diagnostic code may write directly to a terminal.
- Do not repaint the whole screen on every cursor movement. Preserve ordinary
  Ratatui diffs and the existing broad-change/periodic repaint policy.
- Use stdout when it is a terminal, preserving existing behavior.
- When stdout is redirected, use an explicit controlling-terminal writer where
  the platform supports it. Do not silently send UI escape sequences into the
  diff-tool output pipe.
- Keep input, raw-mode setup/restoration, and output on the intended terminal.
  Fail clearly or use an explicitly documented fallback if no controlling
  terminal is available.
- Preserve the one-writer and lock-ordering rules in `AGENTS.md`.
- Keep `frame_log` functional without giving it an independent terminal writer.

## Phases

### 1. Establish current behavior

- [x] Runtime descriptor/source investigation established that `jj diff`
  intentionally pipes generated-diff stdout and relays it, while the editor
  path inherits stdio.
- [x] A pseudo-terminal capture recorded movement-key handling, selection
  change, and incremental output for the affected tree view.

**Status:** the pipe-vs-terminal transport difference is established. The
original stale visual symptom is not independently reproduced in a terminal
emulator, and file-view evidence is still outstanding.

### 2. Design the terminal writer selection

- [x] Select stdout when it is a TTY; otherwise use the platform controlling
  terminal for rendering.
- [x] Keep the backend as the sole UI writer and preserve the frame logger.
- [ ] Further verify input/raw-mode behavior on supported platforms.

**Status:** a Unix/Windows writer-selection prototype is implemented. The
Windows handle path and no-TTY behavior need platform-specific validation.

### 3. Implement the narrow fallback

- [x] Keep frame writes, queued sequences, and terminal restoration on the
  same Ratatui backend.
- [x] Leave the incremental repaint policy unchanged.

**Status:** implementation is in place and the Unix PTY path was exercised.

### 4. Validate and decide

- [x] PTY check: direct-terminal launch enters/exits alternate screen.
- [x] PTY check: `oyui split` still enters/exits alternate screen; quitting
  cancels the split as expected.
- [x] PTY check: piped-stdout launch uses the controlling terminal and emits
  zero UI bytes to the stdout pipe.
- [x] PTY check: `j` is received, dispatched, and advances tree selection in
  the piped-stdout run.
- [x] PTY check: resizing is received during the piped-stdout run and terminal
  teardown restores the alternate screen.
- [x] No-controlling-terminal check: no UI bytes leak to stdout; initialization
  reports the unavailable controlling terminal.
- [ ] Verify a movable file-view cursor and its rendered update.
- [ ] Runtime-test Windows and other supported platforms.
- [ ] Check the original stale-highlight symptom in the user's terminal
  emulator; a PTY byte capture is not visual confirmation.
- [ ] Run the remaining behavior matrix and investigate further if the stale
  visual remains.

**Exit criterion:** behavior is correct in the matrix above, or the experiment
is reverted/reevaluated with evidence explaining why controlling-terminal
output was insufficient.

## Out of scope

- Replacing `jj diff` with Jujutsu's mutating diff-editor workflow.
- Changing Jujutsu's source or relying on undocumented pager behavior.
- Increasing repaint frequency to mask a possible transport issue.
- Claiming that the relay is faulty without evidence of delayed, altered, or
  interleaved bytes.
