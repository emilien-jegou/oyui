# Configuration

Oyui is configured with a [Rune](https://rune-rs.github.io/) script. The
default path is `~/.config/oyui/config.rn`; override it with `oyui -c <path>`
or `--config <path>`.

The script's entrypoint is `config()`. It runs once at startup and again on
every save (hot reload). Compile errors and failed `config()` calls keep the
last working configuration. Registration errors are reported during load;
the invalid config is discarded and the last working configuration remains.

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
keybind::global("ctrl-j", || view::file::cursor::down(5));
```

A chord is built from modifiers joined to a key:

- **Modifiers**: `ctrl` (or `control`), `alt`, `shift`. Combine them, e.g.
  `"ctrl-shift-j"`.
- **Named keys**: `enter`, `esc` (or `escape`), `backspace`, `up`, `down`,
  `left`, `right`, `pageup`, `pagedown`, `tab`, `space`.
- **Any single character**: `"j"`, `"G"`, `":"`, `" "` (prefer `"space"`).

Unknown key names and malformed chords are reported as configuration errors
rather than silently binding the wrong key.

`keybind::in_view` evaluates its closure immediately while registering
view-specific bindings. It is not a callback that runs when the user changes
views. Use `keybind::global` for a binding that applies in every view, or
`keybind::bind` inside a view scope:

```rune
keybind::in_view("file", || {
    keybind::bind("ctrl-j", || view::file::cursor::down(5));
    keybind::bind("ctrl-k", || view::file::cursor::up(5));
});

keybind::in_view("tree", || {
    keybind::bind("ctrl-j", || view::tree::cursor::down(5));
});
```

Views are named `"file"` and `"tree"`; an unknown view is a load error.
`keybind::bind` replaces an existing binding in its scope (it does not add a
second callback). A global binding and an active-view binding can both run for
the same key. A global rebind takes ownership of that key across views; a
view-scoped rebind replaces only that view's binding.

### Naming keybinds

Use `keybind::named(chord, label, callback)` to give a binding a readable
label shown in the help overlay. Unlabeled script bindings appear as
`<script>`. It uses the current registration scope.

```rune
keybind::in_view("file", || {
    keybind::named("ctrl-j", "scroll down 5", || view::file::cursor::down(5));
});
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

Use `keybind::remove_global`, `keybind::remove_from_view`, or
`keybind::remove_everywhere` to remove a chord at the intended scope:

```rune
keybind::remove_from_view("file", "s"); // stop file-view split
keybind::remove_everywhere("space");    // stop staging in both views
```

To drop **all** bindings (built-in and custom) and start from a clean slate:

```rune
keybind::remove_all();
keybind::global("j", || view::file::cursor::down(1)); // only this binding remains
```

To restore the built-in defaults after clearing or overriding them:

```rune
keybind::remove_all();
keybind::restore_defaults(); // back to the default key map
```

`keybind::list()` returns one string per binding (`scope: keys = labels`) and
includes built-in and script bindings.

---

## Commands

Register commands the `:` prompt and `commands::run` can execute:

```rune
commands::register("copy_hunk", |args| {
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
  `commands::run("copy_hunk")`.
- `commands::list()` returns a list of registered script
  commands (built-in palette commands are listed in
  [actions.md](./actions.md#palette-commands)).

---

## Events

React to application events with `events::on`:

```rune
events::on("file_opened", || global::notify("opened " + view::file::path()));
events::on("quit", || {});
```

| Event | When |
|---|---|
| `config_reload` | After a successful configuration reload. |
| `file_opened` | After a file is opened in the file view. |
| `view_changed` | When the active view changes. |
| `quit` | Right before the application exits. |

Callbacks run synchronously on the main thread and take no arguments. Multiple
callbacks run in registration order; a callback failure stops later callbacks
for that event and reports an error. `config_reload` fires only after a
successfully applied configuration. Read context through getters (for example
`view::file::path()`).

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

`analysis::find_files_containing` scans file contents with a
[Rust `regex`](https://docs.rs/regex) pattern:

```rune
// Blocking: runs on the UI thread.
let matches = analysis::find_files_containing("TODO|FIXME");
```

The result is a list of paths (not a newline-joined string). The scan reads
both old and new file contents from disk; unreadable files are skipped. For
large repositories use the async form, which runs on a worker thread and calls
back on the main thread with `(matches, error)`:

```rune
commands::register("commit_regex", |pattern| {
    analysis::find_files_containing_async(pattern, |matches, error| {
        if let Some(message) = error {
            global::error(message);
            return;
        }
        for path in matches {
            view::tree::staging::set(path, true);
        }
        global::notify("staged matching files");
    });
});
```

The synchronous form reports invalid patterns as a transient error and returns
an empty list. The async callback receives an optional error as its second
argument.

---

## Resolving conflicts

Conflicts are shown **inline in the file view**: the working file contains the
`<<<<<<<`/`|||||||`/`=======`/`>>>>>>>` markers as ordinary lines, styled
prominently among the surrounding hunks. There is no separate window. Markers
are read from the target (git's `$MERGED`); if the target is clean, oyui
synthesizes a three-way merge from `base`/`left`/`right` first.

Put the cursor on the side you want and press `space` to fold the conflict
into a frame around that side's lines:

- `space` on ours/theirs content folds with that side (`=======` counts as
  theirs); while the cursor is on a side, that side's marker (`<<<<<<<` for
  ours, `>>>>>>>` for theirs) takes the frame highlight as a preview.
- `space` anywhere on a folded conflict — frame or kept center lines —
  expands back to the markers.

Selecting a side folds the conflict into a frame, `ours ———…` …
`———…`, around that side's lines. Folding is **display-only**:
the working file keeps its markers, so a choice can be revisited and the
conflict can still be confirmed as-is.

Conflict hunks are not stageable: `t`, `split` and in-hunk `space` toggles
are no-ops inside conflicts, and conflict colors never reflect staging, so
identical conflict states always look identical. The written resolution is
driven by choices alone — to take a resolution back, unfold it to the
markers.
`session::conflict_count()` reports how many conflicts were detected.

Conflict choices can also be driven from scripts. Conflict indexes are
zero-based; `choose` accepts `"ours"`, `"theirs"`, or `"both"` and returns
`false` when the index, side, or active resolver is invalid:

```rune
commands::register("take-ours", |args| {
    let count = view::file::conflict::count();
    for index in 0..count {
        view::file::conflict::choose(index, "ours");
        view::file::conflict::set_folded(index, true);
    }
});
```

Orange is the color of conflict, kept subtle: unfolded conflicts render as
plain rows (no green/red/blue, no background wash) with orange marker lines
(`<<<<<<< ours`, …), while the fold frames — and the hovered side's marker
as a preview — carry a tuned-down orange background using the same gradient
logic as staged hunks (gradient by default, solid otherwise). Override the
frame underlay like any other highlight:

```rune
theme::conflict_bg::set("#202030");                 // explicit underlay color
theme::file_conflict_highlight::set(LineHighlightMode::None);      // off
theme::file_conflict_highlight::set(LineHighlightMode::Gradient(0.2));
theme::file_conflict_highlight_opacity::set(0.6);
```

Confirming follows the integration's rules:

- **`oyui git mergetool`** (default): `enter` refuses to confirm while
  conflicts remain, reporting how many are left.
- **`oyui jj mergetool`**: confirming with conflicts still present is allowed —
  jj stores them structurally, so committing a conflict is intended, not an
  error.

In a merge session, confirm never runs the two-way staging write; it writes the
resolved working file, so the merge target is never clobbered.

```rune
events::on("config_reload", || {
    if session::conflict_count() > 0 {
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
