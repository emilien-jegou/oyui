# Script API reference

Oyui's configuration is a [Rune](https://rune-rs.github.io/) script. Every
feature below is called from your `config()` entrypoint or from a callback
registered with `keybind`, `command`, or `on`.

```rune
pub fn config() {
    // runs once at startup and on every hot reload
}
```

- Functions that return nothing may be called directly.
- Functions that return a value are shown with `-> Type`.
- `{ get; set }` means the field has both a getter and a setter.
- Absence is modelled with Rune's `Option`: values that may be missing are
  shown as `-> Option<T>` and matched with `Some(x)` / `None`.

> Looking for the configuration workflow, keybind syntax, events, and command
> registration? See [configuration.md](./configuration.md).
> Default keybindings are listed in [keybindings.md](./keybindings.md).

---

## `global`

Session-level actions and context.

| Function | Returns | Description |
|---|---|---|
| `global::quit()` | | Quit the application. |
| `global::confirm()` | | Confirm the current action; opens the merge window when `confirm_merge_window_enabled` is set. |
| `global::execute_merge()` | | Apply the staged result and exit. |
| `global::open_command_mode()` | | Open the `:` command prompt. |
| `global::confirm_merge_window_enabled` | `bool` | Whether `confirm()` opens the merge confirmation window. |
| `global::left_path()` | `String` | Left/old input path. |
| `global::right_path()` | `String` | Right/new input path. |
| `global::base_path()` | `String` | Base path, or `""` when unset. |
| `global::view()` | `String` | Active pane: `"file"` or `"tree"`. |
| `global::algorithm()` | `String` | Diff algorithm: `histogram`, `myers`, `myersminimal`, `syntaxaware`. |
| `global::operation()` | `String` | Session kind: `diff` or `merge`. |
| `global::writable()` | `bool` | Whether confirming writes a result (`false` for `diff --no-write`). |
| `global::conflict_count()` | `u32` | Conflicts detected in the merge target (0 when none/not a merge). |
| `global::switch(view)` | | Switch pane; `view` is `"file"` or `"tree"`. |
| `global::command(cmd)` | | Run a palette command (see [commands](#palette-commands)). |
| `global::undo()` | | Undo the last staging mutation. |
| `global::redo()` | | Redo the last undone staging mutation. |
| `global::copy(text)` | | Copy `text` to the system clipboard via OSC 52. |
| `global::help()` | | Toggle the keybinding-help overlay. |
| `global::notify(msg)` | | Show a transient info message. |
| `global::warn(msg)` | | Show a transient warning message. |
| `global::error(msg)` | | Show a transient error message. |
| `global::clear_message()` | | Clear the transient message. |
| `global::clear_error()` | | Clear the persistent configuration-error overlay. |

---

## `settings`

| Field | Type | Default | Description |
|---|---|---|---|
| `settings::scrolloff` | `u32` | `2` | Context lines kept above/below the cursor in both panes. |
| `settings::context_lines` | `u32` | `4` | Unchanged lines shown around each hunk in the file view. |

Both apply immediately and affect the tree and file panes.

---

## `analysis`

| Function | Returns | Description |
|---|---|---|
| `analysis::files_containing(pattern)` | `String` | Newline-joined paths whose old **or** new side matches the regex. **Blocking** (runs on the UI thread). |
| `analysis::files_containing_async(pattern, cb)` | | Same, on the worker thread. `cb(matches: String)` receives the newline-joined result. |

`pattern` is a Rust `regex` pattern. Matching reads file contents from disk
(both sides of the diff), not the in-memory diff.

```rune
analysis::files_containing_async("TODO|FIXME", |matches| {
    for path in matches.split("\n") {
        if path != "" { global::warn(path); }
    }
});
```

---

## `ui`

| Function | Returns | Description |
|---|---|---|
| `ui::hint::set(view, format)` | | Replace the hint bar for `view` (`"file"`/`"tree"`). Format: `"key=desc key=desc"`; `_` in a description becomes a space. |
| `ui::hint::clear(view)` | | Restore the built-in hints for `view`. |
| `ui::tree_row_format` | `String` | Space-separated tree-row fields, in order. Fields: `state`, `icon`, `name`, `stats`, `path`. Default: `"state icon name stats"`. |
| `ui::status` | `String` | Persistent status line on the bottom bar (shown when no message or command is active). |

```rune
ui::hint::set("file", "j/k=move space=stage s=split :=cmd");
ui::tree_row_format::set("state name stats");
ui::status::set("oyui");
```

---

## `theme`

### Loading themes

| Function | Returns | Description |
|---|---|---|
| `theme::set(name)` | | Apply a full theme (UI + syntax). `name` is a builtin, `"ansi"`, or `"path:/path/to.tmTheme"`. |
| `theme::syntax(name)` | | Replace only the syntax-highlighting theme. |
| `theme::name()` | `String` | Active builtin name, or `""` if none was selected by name. |
| `theme::list()` | `String` | Newline-separated list of builtin theme names. |
| `theme::is_dark()` | `bool` | Whether the current background is dark. |
| `theme::toggle_gradient()` | | Toggle gradient line rendering. |
| `theme::gradient` | `bool` | Gradient line rendering on/off. |

### Colors

Every color field below is `{ get; set }` and takes/returns a color string.

`bg`, `fg`, `cursor_bg`, `dim`, `dimmer`, `staged`, `unstaged`, `partial`,
`dir`, `cmd`, `add_bg`, `del_bg`, `add_fg`, `del_fg`,
`char_scroll_fg`, `char_trailing_space_fg`, `char_tab_fg`

`char_line_split_color` and `char_hunk_split_color` additionally accept `""`
or `"none"` to clear the override.

**Color string formats**

| Format | Example |
|---|---|
| Hex | `"#ff8800"` (also `"ff8800"`) |
| RGB | `"rgb(255, 136, 0)"` |
| Named | `"red"`, `"lightblue"`, `"reset"`, `"fg"`, `"bg"`, ... |
| Theme field | `"theme:dim"`, `"theme:partial"` |
| Syntax theme field/scope | `"tm:foreground"`, `"tm:markup.inserted"` |
| Indexed | `"ansi:4"`, `"ansi256:208"` |

> Named values include: `reset`/`default`, `bg`, `fg`, `black`, `red`,
> `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`/`grey`,
> `darkgray`/`darkgrey`, `lightred`, `lightgreen`, `lightyellow`,
> `lightblue`, `lightmagenta`, `lightcyan`, `white`.

### Highlight modes

| Field | Type | Description |
|---|---|---|
| `theme::file_staged_highlight` | `LineHighlightMode` | Staged-line highlight: `LineHighlightMode::None`, `::Solid`, `::Gradient(f64)`. |
| `theme::file_staged_highlight_opacity` | `f64` | Gradient opacity for staged lines. |
| `theme::file_change_highlight` | `LineHighlightMode` | Changed-line highlight. |
| `theme::file_change_highlight_opacity` | `f64` | Gradient opacity for changed lines. |

### Glyphs and strings

| Field | Default | Description |
|---|---|---|
| `theme::char_indicator` | `"▎"` | Change indicator. |
| `theme::char_add_sign` | `"+ "` | Addition sign. |
| `theme::char_del_sign` | `"- "` | Deletion sign. |
| `theme::char_hunk_split` | `"◣"` | Hunk-split marker. |
| `theme::char_line_split` | `"▶"` | Line-toggle marker. |
| `theme::char_trailing_space` | `"•"` | Trailing whitespace glyph. |
| `theme::char_tab` | `"·   "` | Tab glyph. |
| `theme::char_scroll_both` | `"↔"` | Horizontal-scroll indicator (both). |
| `theme::char_scroll_left` | `"˂"` | Horizontal-scroll indicator (left). |
| `theme::char_scroll_right` | `"˃"` | Horizontal-scroll indicator (right). |
| `theme::tree_progressive_change_dim` | `false` | Dim small tree change counts. |

```rune
theme::set("weywot");
theme::bg::set("#000000");
theme::partial::set("theme:cmd");
theme::file_staged_highlight::set(LineHighlightMode::Gradient(0.05));
theme::gradient::set(true);
```

---

## `view::file`

### Scrolling

| Function | Description |
|---|---|
| `view::file::scroll::left(n)` | Scroll horizontally left by `n`. |
| `view::file::scroll::right(n)` | Scroll horizontally right by `n`. |

### Cursor

| Function | Returns | Description |
|---|---|---|
| `view::file::cursor::up(n)` / `down(n)` | | Move the cursor by `n` visual rows. |
| `view::file::cursor::page_up()` / `page_down()` | | Move by one page. |
| `view::file::cursor::half_page_up()` / `half_page_down()` | | Move by half a page. |
| `view::file::cursor::top()` / `bottom()` | | Jump to the first/last row. |
| `view::file::cursor::row()` | `u32` | Cursor visual row. |
| `view::file::cursor::row_count()` | `u32` | Total visual rows. |
| `view::file::cursor::hunk_index()` | `Option<u32>` | Hunk under the cursor, or `None`. |
| `view::file::cursor::hunk_count()` | `u32` | Number of hunks. |
| `view::file::cursor::kind()` | `String` | Line kind: `context`, `addition`, `deletion`, or `none`. |
| `view::file::cursor::text()` | `String` | Text of the line under the cursor. |
| `view::file::cursor::old_line()` / `new_line()` | `u32` | Old/new line number (0 when not applicable). |
| `view::file::cursor::is_staged()` | `bool` | Whether the line under the cursor is staged. |

### Hunk navigation

| Function | Description |
|---|---|
| `view::file::nav::next_hunk()` | Jump to the next hunk. |
| `view::file::nav::prev_hunk()` | Jump to the previous hunk. |

### Staging

| Function | Returns | Description |
|---|---|---|
| `view::file::staging::toggle()` | | Toggle staging for the hunk at the cursor. |
| `view::file::staging::toggle_hunk(i)` | | Toggle hunk `i`. |
| `view::file::staging::toggle_line()` | | Toggle staging for the line at the cursor. |
| `view::file::staging::split()` | | Split the hunk at the cursor. |
| `view::file::staging::split_at(hunk, line)` | | Split hunk `hunk` at line index `line` within it. |
| `view::file::staging::join()` | | Join the hunk at the cursor with the previous one. |
| `view::file::staging::set_hunk(hunk, staged)` | | Deterministically stage/unstage hunk `hunk`. |
| `view::file::staging::stage_all()` / `unstage_all()` | | Stage/unstage every modifiable line in the file. |
| `view::file::staging::invert()` | | Invert all selections. |
| `view::file::staging::state()` | `String` | File staging state: `staged`, `partial`, `unstaged`, `none`. |

### Folding and file

| Function | Returns | Description |
|---|---|---|
| `view::file::fold::toggle()` | | Collapse/expand unchanged context. |
| `view::file::folded()` | `bool` | Unchanged context collapsed. |
| `view::file::path()` | `String` | Open file path, or `""`. |
| `view::file::close()` | | Return to the tree view. |

### Inspection

| Function | Returns | Description |
|---|---|---|
| `view::file::inspect::hunk_has()` | `bool` | Whether a hunk is under the cursor. |
| `view::file::inspect::hunk_index()` | `Option<u32>` | Hunk under the cursor, or `None`. |
| `view::file::inspect::hunk_count()` | `u32` | Number of hunks. |
| `view::file::inspect::hunk_text()` | `String` | Newline-joined text of the hunk under the cursor. |
| `view::file::inspect::hunk_marker()` | `String` | `none`, `linetoggle`, or `hunksplit`. |
| `view::file::inspect::hunk_is_staged()` | `bool` | Whether the current hunk is fully staged. |
| `view::file::inspect::diff_text()` | `String` | Unified diff of the open file. |
| `view::file::inspect::new_text()` | `String` | Full new-side content. |
| `view::file::inspect::old_text()` | `String` | Full old-side content. |
| `view::file::inspect::staged_text()` | `String` | Content produced by the current selections. |
| `view::file::inspect::stats()` | `String` | `"+N -M"` (or `"binary N"`). |
| `view::file::inspect::is_binary()` | `bool` | Whether the open file is binary. |

```rune
command("copy_hunk", |args| {
    if let Some(_hunk) = view::file::cursor::hunk_index() {
        global::copy(view::file::inspect::hunk_text());
        global::notify("hunk copied");
    } else {
        global::error("no hunk under cursor");
    }
});
```

---

## `view::tree`

### Cursor

`view::tree::cursor::up(n)`, `down(n)`, `page_up()`, `page_down()`,
`half_page_up()`, `half_page_down()`, `top()`, `bottom()`.

### Directories

| Function | Description |
|---|---|
| `view::tree::directory::expand()` / `collapse()` | Expand/collapse the selected directory. |
| `view::tree::directory::expand_all()` / `collapse_all()` | Expand/collapse every directory. |

### Staging

| Function | Returns | Description |
|---|---|---|
| `view::tree::staging::toggle_selected()` | | Toggle staging for the selected row. |
| `view::tree::staging::set(path, staged)` | | Set a path's staging state. |
| `view::tree::staging::set_matching(glob, staged)` | | Set staging for every file matching `glob`. |
| `view::tree::staging::stage_all()` / `unstage_all()` | | Stage/unstage every file. |
| `view::tree::staging::invert()` | | Invert every file's staging state. |

### Opening

| Function | Description |
|---|---|
| `view::tree::open_selected()` | Open the selected file or toggle the selected directory. |
| `view::tree::open_file(path)` | Open `path` in the file view. |

### Selection context

| Function | Returns | Description |
|---|---|---|
| `view::tree::selected_index()` | `u32` | Selected flat-row index. |
| `view::tree::row_count()` | `u32` | Number of visible rows. |
| `view::tree::selected_path()` | `String` | Selected row path. |
| `view::tree::selected_name()` | `String` | Selected row name. |
| `view::tree::selected_is_dir()` | `bool` | Whether the selection is a directory. |
| `view::tree::selected_staging()` | `String` | Selection staging state. |
| `view::tree::is_folded(path)` | `bool` | Whether `path` is collapsed. |

### Inspection

| Function | Returns | Description |
|---|---|---|
| `view::tree::inspect::files()` | `String` | Newline-joined paths of all files. |
| `view::tree::inspect::path_at(i)` | `String` | Path of flat row `i`. |
| `view::tree::inspect::is_dir_at(i)` | `bool` | Whether row `i` is a directory. |
| `view::tree::inspect::staging_at(i)` | `String` | Staging state of row `i`. |
| `view::tree::inspect::file_diff_text(path)` | `String` | Unified diff for `path` if cached, else `""`. |
| `view::tree::inspect::file_stats(path)` | `String` | `"+N -M"` for `path`. |
| `view::tree::inspect::file_staging(path)` | `String` | Staging state of `path`. |

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

---

## Palette commands

Run with `global::command(cmd)` or from the `:` prompt. Script commands
registered with `command(...)` are also reachable from the prompt.

| Command | Description |
|---|---|
| `invert` / `i` | Invert staging for every file. |
| `add <glob>` / `a <glob>` | Stage every file matching the glob. |
| `unstage <glob>` / `u <glob>` | Unstage every file matching the glob. |
| `<script-name> [args]` | Invoke a command registered with `command(name, ...)`. |

```rune
global::command("add **/*.md");
```

---

## Default keybindings

Set by the built-in registry and overridable with `keybind`/`unbind`.
See [keybindings.md](./keybindings.md) for the full table.
