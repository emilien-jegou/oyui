# Configuration

Oyui is configured with a [Rune](https://rune-rs.github.io/) script. The
default path is `~/.config/oyui/config.rn`; override it with `oyui -c <path>`
or `--config <path>`.

The script's entrypoint is `config()`. It runs once at startup and again on
every save (hot reload); failed reloads keep the previous keybinds and show the
compiler error in an overlay.

```rune
pub fn config() {
    theme::set("weywot");
}
```

See [actions.md](./actions.md) for every function, and
[keybindings.md](./keybindings.md) for the defaults.

---

## Keybinds

```rune
keybind("ctrl-j", || view::file::cursor::down(5));
```

A chord is built from modifiers joined to a key:

- **Modifiers**: `ctrl` (or `control`), `alt`, `shift`. Combine them, e.g.
  `"ctrl-shift-j"`.
- **Named keys**: `enter`, `esc` (or `escape`), `backspace`, `up`, `down`,
  `left`, `right`, `pageup`, `pagedown`, `tab`, `space`.
- **Any single character**: `"j"`, `"G"`, `":"`, `" "` (prefer `"space"`).

Unknown key names and malformed chords are reported as configuration errors
rather than silently binding the wrong key.

Binds are **global** unless wrapped in `on_mode`:

```rune
on_mode("file", || {
    keybind("ctrl-j", || view::file::cursor::down(5));
    keybind("ctrl-k", || view::file::cursor::up(5));
});

on_mode("tree", || {
    keybind("ctrl-j", || view::tree::cursor::down(5));
});
```

`on_mode` accepts only `"file"` and `"tree"`; anything else is a load error.

### Naming keybinds

Use `keybind_named(chord, label, callback)` to give a bind a readable label
shown in the help overlay. Plain `keybind` binds appear as `<script>`.

```rune
keybind_named("ctrl-j", "scroll down 5", || view::file::cursor::down(5));
```

### Seeing all keybinds

Press `?` (or run `:help keybinds`) to open a scrollable overlay listing every
binding — global plus both views — including bindings registered by your
config. The active view is marked `(current)`.

| Key (in overlay) | Action |
|---|---|
| `?`, `q`, `esc` | Close |
| `j`/`k`, `down`/`up` | Scroll |
| `d`/`u`, `page down`/`page up` | Scroll by 10 |
| `g`/`G`, `home`/`end` | Top / bottom |

### Removing default binds

`unbind` removes every binding for a chord (in all modes). It prunes the chord
from multi-key sources without dropping its siblings:

```rune
unbind("s");       // stop the default split binding
unbind("space");   // stop the default stage binding
```

Registering a new callback on an already-bound chord **adds** to it; use
`unbind` first to replace.

To drop **all** binds (core and custom) and start from a clean slate:

```rune
unbind_all();
keybind("j", || view::file::cursor::down(1));   // only this binding remains
```

To restore the built-in defaults after clearing or overriding them:

```rune
unbind_all();
reset_keybinds();   // back to the default key map
```

---

## Commands

Register commands the `:` prompt (and `global::command`) can run:

```rune
command("copy_hunk", |args| {
    if !view::file::inspect::hunk_has() {
        global::error("no hunk under cursor");
        return;
    }
    global::copy(view::file::inspect::hunk_text());
    global::notify("hunk copied");
});
```

- The callback receives the raw, untyped argument string (everything after the
  command name).
- Run it as `:copy_hunk` or from a keybind with
  `global::command("copy_hunk")`.
- `command_names()` returns a newline-separated list of registered script
  commands (built-in palette commands are listed in
  [actions.md](./actions.md#palette-commands)).

---

## Events

React to application events with `on`:

```rune
on("file_opened", || global::notify("opened " + view::file::path()));
on("quit", || {});
```

| Event | When |
|---|---|
| `config_reload` | After a successful configuration reload. |
| `file_opened` | After a file is opened in the file view. |
| `view_changed` | When a keybind switches the active pane. |
| `quit` | Right before the application exits. |

Callbacks are invoked on the main thread and take no arguments; read context
through the getters (for example `view::file::path()`).

---

## Notifications

| Function | Use |
|---|---|
| `global::notify(msg)` | Transient info on the bottom bar. |
| `global::warn(msg)` | Transient warning. |
| `global::error(msg)` | Transient error. |
| `global::clear_message()` | Dismiss the message. |
| `global::clear_error()` | Dismiss the persistent config-error overlay. |

Messages are flattened to a single line and expire after a few seconds.
Fatal configuration problems (compile errors) use the persistent overlay
instead.

---

## Staging and undo

Staging is captured for undo automatically:

```rune
view::file::staging::toggle();      // then press u to undo
```

`global::undo()` / `global::redo()` (bound to `u` and `ctrl-r`) restore tree
staging state and every cached diff selection.

For scripted bulk staging:

```rune
view::tree::staging::set("src/main.rs", true);
view::tree::staging::set_matching("**/*.md", true);
view::file::staging::stage_all();
```

Bulk operations record a single undo point.

---

## Analysis

`analysis::files_containing` scans file contents with a
[Rust `regex`](https://docs.rs/regex) pattern:

```rune
// Blocking: runs on the UI thread.
let matches = analysis::files_containing("TODO|FIXME");
```

For large repositories use the async form, which runs on the worker thread and
delivers results to a callback on the main thread:

```rune
command("commit_regex", |pattern| {
    analysis::files_containing_async(pattern, |matches| {
        for path in matches.split("\n") {
            if path != "" { view::tree::staging::set(path, true); }
        }
        global::notify("staged matching files");
    });
});
```

An invalid pattern is reported through `global::error`.

---

## Resolving conflicts

Conflicts are shown **inline in the file view**: the working file contains the
`<<<<<<<`/`|||||||`/`=======`/`>>>>>>>` markers as ordinary lines, styled
prominently among the surrounding hunks. There is no separate window. Markers
are read from the target (git's `$MERGED`); if the target is clean, oyui
synthesizes a three-way merge from `base`/`left`/`right` first.

Put the cursor on a conflict and choose a side:

| Key | Action |
|---|---|
| `o` | Take ours |
| `T` | Take theirs |
| `B` | Take both |

Resolving rewrites the working file and recomputes the diff in place, so the
markers disappear as you go. `global::conflict_count()` reports how many
conflicts were detected.

Confirming follows the integration's rules:

- **git / mergetool** (default): `enter` refuses to confirm while conflicts
  remain, reporting how many are left.
- **jj resolve**: pass `--allow-unresolved` (see the jj config in the README)
  to permit confirming with conflicts left.

In a merge session, confirm never runs the two-way staging write; it writes the
resolved working file, so the merge target is never clobbered.

```rune
on("config_reload", || {
    if global::conflict_count() > 0 {
        global::warn("conflicts remain");
    }
});
```

## Clipboard

`global::copy(text)` copies via OSC 52, which works over SSH and needs no
external program:

```rune
global::copy(view::file::inspect::hunk_text());
```

---

## Theme overrides

```rune
theme::set("weywot");                       // full theme by name
theme::set("path:/home/me/my.tmTheme");     // load a tmTheme file
theme::set("ansi");                         // terminal-default colors

theme::bg::set("#000000");
theme::file_staged_highlight::set(LineHighlightMode::Gradient(0.05));
theme::char_indicator::set("▎");
```

See [actions.md](./actions.md#theme) for the full list and the color string
formats.

---

## UI options

```rune
ui::hint::set("file", "j/k=move space=stage s=split :=cmd");
ui::tree_row_format::set("state icon name stats");
ui::status::set("oyui");
settings::scrolloff::set(3);
settings::context_lines::set(6);
```

---

## Error handling

- **Compile errors** and failed `config()` runs show the persistent overlay and
  keep the last working keybinds.
- **Runtime failures** (keybind callback errors, invalid arguments, bad
  regexes) appear as a transient error message and are written to the log.

Enable logging with `--log-enable --log-console` for details.

---

## LSP

A built-in language server provides completion and diagnostics for `.rn`
config files:

```sh
oyui language-server
```

See the [README](../README.md#enabling-config-lsp-with-neovim) for a Neovim
setup.
