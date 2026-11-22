# Stale cursor highlighting in `oyui diff`

## Summary

Cursor highlighting can remain stale in `oyui diff`; resizing the terminal
repairs it. The same repository and terminal behave correctly in `oyui split`.
The symptom is reported in both tree and file views.

The evidence collected so far points away from oyui's cursor state and
Ratatui's buffer-diff calculation for the tested tree-view movement. The
verified difference is the stdout path used by Jujutsu:

- In `oyui diff`, the embedded oyui process writes stdout to a pipe. The
  enclosing `jj` process has the terminal on stdout and relays the tool output.
- In `oyui split`, the embedded oyui process writes stdout directly to the
  terminal.

This makes the Jujutsu output-relay / terminal-control path the leading area to
investigate. The relay has not been shown to corrupt or lose bytes, so it is
not yet a proven root cause.

## Reproduction

In the affected repository, compare:

```sh
oyui diff
oyui split
```

For each run, wait until the UI is visible, wait one second, press `j` once,
wait one second, then press `q`. The reported stale highlight is visible in
both tree and file views in `oyui diff`; `oyui split` behaves correctly.

Quitting split with `q` cancels the split; the resulting nonzero tool exit is
expected for that test and does not indicate a cursor-probe failure.

## Evidence

### Cursor state and rendered output

The captured `oyui diff` and `oyui split` frame logs show:

- The `j` key was received and dispatched promptly in both sessions.
- Tree selection changed from index `0` to `1` in both.
- The frame after `j` used an incremental diff (`reset_diff=false`).
- The rendered buffer diff changed the old row back to the normal background
  and assigned the cursor background to the new row.
- The captured ANSI stream contains updates for both rows with those
  backgrounds.

This rules out a missed `j` event, an unchanged tree selection, or Ratatui's
buffer diff omitting the highlight transition in this tested tree-view frame.
The frame log records bytes written by oyui's stdout writer; for `oyui diff`,
that writer is a pipe, not the terminal.

### Jujutsu process and descriptor inspection

Runtime inspection was performed with the installed Jujutsu binary,
`jj 0.47.1`, in a pseudo-terminal:

| Invocation | Process | stdin | stdout | stderr |
|---|---|---|---|---|
| `oyui diff` | outer `jj` | terminal | terminal | terminal |
| `oyui diff` | embedded oyui | terminal | **pipe** | terminal |
| `oyui split` | outer `jj` | terminal | terminal | terminal |
| `oyui split` | embedded oyui | terminal | **terminal** | terminal |

A syscall trace showed the outer `jj` writing screen-sized chunks to its
terminal while the embedded oyui wrote to the pipe. This is consistent with
Jujutsu relaying the diff tool's output.

Jujutsu `v0.47.1` source confirms the distinction. In
[`cli/src/merge_tools/external.rs`](https://github.com/jj-vcs/jj/blob/v0.47.1/cli/src/merge_tools/external.rs),
`invoke_external_diff()` explicitly sets the external diff generator's stdout
to `Stdio::piped()`, spawns it, then copies `child.stdout` into the formatter's
writer with `io::copy()`. Its stderr is set to `ui.stderr_for_child()`.
`DiffRenderer::show_diff_trees()` in
[`cli/src/diff_util.rs`](https://github.com/jj-vcs/jj/blob/v0.47.1/cli/src/diff_util.rs)
passes the raw formatter writer to `generate_diff()`, so for `jj diff --tool`
the pipe is relayed through Jujutsu's stdout formatter to the terminal.

The diff-editor route is different: `edit_diff_external()` in the same
`external.rs` invokes the editor with `Command::status()` and does not pipe its
stdio. The split command gets a `DiffSelector` and calls its editor path; the
runtime descriptor probe confirmed that the resulting oyui process has stdout
attached directly to the terminal. In short, the pipe is not accidental: it is
the source-defined behavior for generated diff output, while interactive
editors inherit stdio.

`jj diff`'s command implementation in
[`cli/src/commands/diff.rs`](https://github.com/jj-vcs/jj/blob/v0.47.1/cli/src/commands/diff.rs)
renders via `ui.stdout_formatter()` and requests a pager. The source therefore
explains the stdout pipe and relay; it does not establish that the relay loses,
reorders, or corrupts bytes. This matches, rather than extends, the runtime
evidence.

The TUI's terminal setup sequences are issued by the same oyui code in both
cases. However, under `jj diff`, the alternate-screen and drawing sequences
are sent to a pipe before Jujutsu relays them. This is not the same terminal
I/O path as split.

## What is established, and what is not

Established:

- oyui receives the movement key and updates its cursor state.
- Ratatui computes the expected changed cells for the observed tree transition.
- The captured terminal sequences contain the expected old-row and new-row
  styles.
- Jujutsu gives the embedded `oyui diff` process a piped stdout, unlike the
  `oyui split` process.

Not established:

- Whether Jujutsu's relay changes, delays, or interleaves any bytes in the
  user's failing run.
- Whether the terminal emulator mishandles a correctly relayed sequence.
- Whether the same cursor-frame evidence has been collected for file view.
  The symptom is reported there too, but the supplied frame logs were from
  tree view.
- Whether a fix that writes directly to the controlling terminal is portable
  and compatible with every supported VCS and platform.

## Likely direction

Avoid treating this as a Ratatui repaint problem until the Jujutsu transport
path is ruled out. A TUI launched as a diff formatter is being given a pipe for
stdout. One candidate is to give interactive rendering an explicit
controlling-terminal writer when stdout is not a TTY, while keeping all TUI
writes inside the Ratatui backend. This should be validated against Jujutsu's
relay behavior and terminal restoration before being adopted.

Using Jujutsu's diff-editor command for a read-only viewer is another
possibility, but it repurposes a normally mutating command and needs careful
no-write and exit-status handling.

## Introspection scope

The investigation inspected the installed Jujutsu version's help output,
observed process file descriptors, traced syscalls in a pseudo-terminal, and
read the matching upstream `v0.47.1` source for `diff`, `split`, diff
formatting, and external tool invocation. The source confirms why stdout is
piped for a generated diff and inherited for an editor. It does **not** prove
that the relay causes the visible stale highlight.

## Controlling-terminal fallback experiment

The event loop selects stdout when it is a terminal and otherwise opens the
controlling terminal (`/dev/tty` on Unix, `CONOUT$` on Windows) for the
Ratatui backend. The frame-logging writer only wraps the selected handle for
diagnostics; frame output, queued terminal sequences, and teardown remain
serialized through the same backend. Input and raw-mode handling are unchanged
and continue to use crossterm's stdin path.

Validation in a pseudo-terminal with the built oyui binary:

- A normal direct-terminal `oyui diff` entered and left the alternate screen.
- With stdout piped and a controlling terminal available, `oyui diff` still
  entered and left the alternate screen, while the enclosing process's stdout
  pipe received zero bytes.
- In that piped run, `j` was read and dispatched; the tree selection advanced
  from 0 to 1 and the frame log recorded the expected incremental ANSI update.
- A resize event was also received during a piped-stdout run, followed by
  successful alternate-screen teardown.
- File view was entered through the same fallback, but the selected file did
  not provide a cursor movement in this probe; file-view cursor behavior is
  still unverified.
- With no controlling terminal, UI bytes did not leak to stdout; terminal
  initialization reported the failed `/dev/tty` open.

This verifies the transport selection and its end-to-end behavior in the
tested Unix PTY setup. It does not establish that the original visible failure
was caused by Jujutsu buffering, nor does a PTY byte capture replace checking
the rendered screen in the user's terminal emulator. File-view cursor
movement and Windows behavior have not been runtime-tested.
