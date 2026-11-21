# Bug report — input lag / UI trailing keybinds

**Status:** open. All in-process layers instrument-exonerated on the user's own
repro; the unmeasured layer below the process (terminal emulator / display
path) is the prime suspect, plus two secondary hypotheses. This file tracks
everything tried so far so nothing gets re-litigated.

---

## 1. Symptom

- After pressing a key, **no visible screen change** — even when waiting
  indefinitely.
- Re-pressing makes the UI move **one step** — but that is the **previous**
  key's effect taking effect.
- I.e. the UI trails the keybinds; a permanent one-event-behind pipeline that
  never catches up, no matter how long you wait.
- Originally reported as "**diff only, not on split**"; later clarified to
  occur in **both views** (tree *and* file pane).
- Repro repository: oyui's own repo (jj-managed).

Repro command (top-level):

```sh
oyui diff -r nwx
```

Important topology: the top-level process **never runs the TUI**. It detects
the enclosing jj repo and delegates (`ToolInvocation::run`):

```
oyui diff -r nwx
  └─ jj --no-pager --config merge-tools.oyui.* diff --tool oyui
       └─ oyui jj edittool $left $right      ← THE TUI (child process)
```

The child is spawned with stdio inherited; before the session's env fix
(`OYUI_LOG_ENABLE`/`OYUI_LOG_PATH`), the child had **no logger at all**, which
is why early log captures watched the delegator and saw nothing.

## 2. Environment

| Component | Version(s) |
|---|---|
| OS | Linux (user desktop; tested inside and outside tmux) |
| rustc | 1.101.0-nightly (2026-10-02) |
| crossterm | **0.28.1 and 0.29.0 — symptom identical on both** |
| ratatui | 0.30.0 |
| mio | 1.2.0 |
| runtime | `#[tokio::main]` (multi-thread) |
| user config | `~/.config/oyui/config.rn` (Rune script; compiles at startup, registers keybinds — relevant: finding 6.11 multi-fire semantics) |
| runtime: tokio | — |

Upstream crossterm issues reviewed and ruled non-matching:

- **#839** — `poll(Duration::ZERO)` returning false with buffered events.
  Requires the `use-dev-tty` feature; we don't enable it.
- **#1039** — EventStream vs third-party stdin readers eating bytes. Needs a
  competing stdin reader; none in this loop.

## 3. Timeline of attempts & evidence

### 3.1 Full static audit — clean

Every file on the tty→pixel path was read this session:

- `app/run.rs` (event loop, select, drain, teardown)
- `app/input.rs` (dispatch), `commons/input.rs` (keybind matching)
- `app/draw.rs`, `view/mod.rs`, `view/tree.rs`, `view/file/render/*`
  (`rows.rs` two-pass build, `line/mod.rs`, `line.rs` renderer, `mod.rs`
  render entry, devicon, style tables)
- `view/file/view_model.rs` (single-slot layout, `LayoutKey`, revision keys)
- `worker/*` (`cpu.rs` bounded rayon pool w/ UI headroom, tasks, coalescing
  cells in `oyui-tasker` incl. their tests), generated registry (`derive/registry.rs`),
- `script/*` (compile off-thread, parked reload, natives),
- `config/*`, `commands/compare.rs` + `standalone.rs` + `cli.rs`, `vcs/*`.

No static defect found that could produce a one-key-behind lag; handling and
paint measured µs–ms class at every hop.

### 3.2 Probe 1 — invalid (broken args / no repo)

`oyui diff <dirs>` expects *paths*, not two trees; also ran outside a repo →
`not_a_repo` exit before any TUI. Result: unusable, app exited pre-loop.

### 3.3 Trace 1 — poll(250ms) reader thread (crossterm 0.29)

```
09.285512 Entering main event loop
09.404868 worker  draw_ms=0.7
10.356804 input   key_ms=1071  draw_ms=8.4
11.004766 input   key_ms=656   draw_ms=9.2   pending=3   (eth file just opened)
11.118594 worker  draw_ms=112.3            (file-open recompute)
11.731055 input   key_ms=735   draw_ms=13.1
12.119208 input   key_ms=401   draw_ms=9.4
12.551371 input   key_ms=441   draw_ms=10.1
```

Iteration gaps cluster at ~225 ms / ~450 ms multiples (= the 250 ms poll tick
minus the draw). Originally read as "`poll` never wakes on input" → motivated
switching to the event-stream.

### 3.4 EventStream rewrite — symptom persists (user-verbal, one run)

Reader task → EventStream, still forwarded through the unbounded mpsc relay.
Same one-word verdict from the user.

### 3.5 crossterm bisect — 0.28.1 pin — persists (user, in + outside tmux)

Pinned `=0.28.1` with the `event-stream` feature; symptom identical inside
tmux and outside. The 0.28→0.29 delta and tmux are therefore **not** the
carrier.

### 3.6 Final instrumented run (0.28, EventStream + frame log)

New instrumentation (`commons/frame_log.rs` + wiring):

- per-key arrival lines (`src=key`, epoch ms),
- per-iteration post-paint scroll-state lines (`src=scroll`),
- **every terminal flush** with instant + byte count + 160-byte snippet
  (`CountingStdout` wrapping the backend writer).

Raw file: 48,497 lines (48,477 flush; the rest key/scroll lines).

**Instrument bugs found while reading (both mine):**

1. `tail -80` truncation of a 31k-line file produced a **false "zero
   flushes"** conclusion. Corrected: flushes happen, ~660–692 cells per
   frame, incremental and real.
2. The relay reader logged `read_at` as the **loop-entry instant** (not the
   event-resolve instant), inflating `key_ms` (1071/656/735/401/441).
   Recomputed from the absolute epoch lines, real key→paint = **9–14 ms**.

### 3.7 "Fix" attempt — direct EventStream branch + noop-drain — **broke the app**

Removed the relay task/mpsc; `events.next()` became a direct `select!`
branch; after each key, a **noop-waker drain** polled `poll_next` without
parking to coalesce bursts.

**Result: the loop parked forever (user had to kill the process).**
Mechanism (from crossterm `stream.rs`, verified by reading):

- Every `Pending` poll of `EventStream` does a `compare_exchange` and sends a
  `Task { waker = cx.waker() }` to its worker thread.
- The drain polled with **`Waker::noop()`**, so the registered Task's wake was
  a no-op.
- The loop's next poll then saw `executed == true`, its CAS failed, and no
  Task carrying the *real* waker was ever sent again → lost wakeup → the loop
  parks on the next keypress forever.

### 3.8 Fix v2 — drain removed — **symptom persists** (user: "error persist")

One event per select iteration, immediate draw; crossterm unpinned back to
0.29; teardown via `drop(events)` (EventStream's `Drop` signals its worker).
Builds clean (debug + release).

The user re-ran with instrumentation still live. Cross-correlated result
(frames epochs ↔ tracing wall-clock):

| Key event | Delivered | Painted | key→paint | Selection |
|---|---|---|---|---|
| 7 cursor moves | 22.030→23.026 | 22.039→23.038 | **9–14 ms each** | 0→7, one per key, in order |
| file open (enter) | ~21.37 | 21.383 (+112 ms recompute frame) | ~1.3 s **once** | resets to 0 |
| final `q` | 23.229 | — | — | quit |

**Every in-process measurement is now exonerated on the user's own real
repro:** input delivery, dispatch (script callbacks included in that span),
state mutation, row model, and the flushed bytes.

**Open oddity:** the enter key that opened the file has **no `src=key` line**
— the event that triggered `Opened selected` never passed through the logged
path. Unexplained; the only key-handling evidence gap remaining.

## 4. What is exonerated (and by what)

| Layer | Method | Verdict |
|---|---|---|
| App event loop (select/drain/dispatch) | instrumented: handle+paint ≈ 0.15 ms | clean |
| Paint path | `draw_ms` 3–16 ms; 112 ms once (open recompute) | clean |
| Workers / parser / syntax | bounded pool w/ UI headroom, off-thread, off-loop | clean |
| Coalescing (oyui-tasker cells) | code + existing interleaving tests | clean |
| crossterm 0.28 vs 0.29 | pinned build, real repro | identical |
| tmux vs no-tmux | real repro | identical |
| Reader design (relay+poll / relay+stream / direct stream) | three builds, real repro | identical |
| **Everything measured in-process** | frame log, user's own hands | **9–14 ms key→bytes, correct renders** |

## 5. Surviving hypotheses (ranked)

1. **Terminal emulator / display path** — the bytes ratatui hands to stdout
   are correct and ≤14 ms per key; what happens between `write(2)` and
   visible pixels is unmeasured. The only layer every in-process instrument
   cannot reach.
2. **User config script (`config.rn`) keybinds** — finding 6.11: script
   bindings and default bindings both fire per matching key (multi-fire).
   The measured 9–14 ms span *includes* any callback that fired, so this is
   weak — but excluding the config altogether is a 10-second test.
3. **ratatui 0.30 buffer/swap behavior** — reaches paint-level exotica
   (empty-ish diffs); contradicted by the observed ~690-cell incremental
   flushes, but never independently verified upstream.

## 6. Proposed next steps

1. **Identify the terminal emulator** and retry once in a different one
   (`xterm`, a plain console, etc.). Decisive for hypothesis 1.
2. **Exclude the script config**: `mv ~/.config/oyui/config.rn{,.bak}`,
   retest. Decisive for hypothesis 2.
3. If both are clean, file upstream with the `.frames` capture: it shows
   every key landing ~10 ms before a correct incremental paint on the exact
   repro — the strongest artifact we produced.
4. If the emulator *is* the culprit, consider compensating at our layer
   (keyboard-enhancement protocol negotiation, explicit flush pacing, or an
   out-of-band repaint trigger) after reproduction there.

## 7. Code changes made during this investigation

- `crates/oyui/src/main.rs` — file log also honors `OYUI_LOG_ENABLE` /
  `OYUI_LOG_PATH` (children spawned by jj inherit env, not our CLI flags);
  arms `frame_log`.
- `crates/oyui/src/commons/frame_log.rs` *(new)* — static frame/flush logger
  + `CountingStdout` (io::Write wrapper logging each terminal write).
- `crates/oyui/src/app/run.rs` — EventStream is now a direct `select!`
  branch (no relay task, no mpsc hop); one event per iteration + immediate
  draw; teardown via `drop(events)`; per-key and per-iteration frame lines.
- `crates/oyui/Cargo.toml` + root — `crossterm` gains `event-stream`;
  `futures-lite` added; the 0.28 bisect pin was added then removed.

## 8. Artifacts

- `/tmp/oyui-final.log` — tracing log of the last runs
- `/tmp/oyui-final.log.frames` — frame/flush/key telemetry
- `/tmp/oyui-028.log`, `/tmp/oyui-nt.log` — bisect runs (verbal results only)
- `/tmp/opencode/xpan/` — 30-line crossterm-only input probe (built, unused)
