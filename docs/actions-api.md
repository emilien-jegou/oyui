# Actions API Documentation

This document describes the action namespace hierarchy exposed to the configuration engine. These actions can be called directly or registered to keybindings.

---

## 1. Global Actions (`global`)
These actions are globally accessible across the application.

| Action | Parameters | Description |
| :--- | :--- | :--- |
| `global::quit()` | None | Terminate the application. |
| `global::confirm()` | None | Confirm the current action or dialog selection. |
| `global::open_command_mode()` | None | Open the command-line/console interface bar. |

---

## 2. Theme Configuration (`theme`)
The `theme` namespace configures UI styling and terminal coloring. Properties marked with `@getset` function as reactive configuration fields.

### Style Toggles
* **`theme::toggle_gradient()`**
  * Type: `String` property
  * Description: Enables or disables gradient rendering for specific UI components.

### Color Properties (Getter/Setters)
These properties accept and return color values as hex strings (e.g., `"#ffffff"`).

All those method also have getteer version that take 0 arguments, e.g. `theme::bg::get()`.

| Property | Type | Description |
| :--- | :--- | :--- |
| `theme::bg::set` | `String` | Default background color. |
| `theme::fg::set` | `String` | Default foreground text color. |
| `theme::cursor_bg::set` | `String` | Color of the cursor background highlight. |
| `theme::dim::set` | `String` | Color representing dimmed or inactive text. |
| `theme::staged::set` | `String` | Text color highlighting fully staged files/changes. |
| `theme::unstaged::set` | `String` | Text color highlighting unstaged files/changes. |
| `theme::partial::set` | `String` | Text color highlighting partially staged items. |
| `theme::dir::set` | `String` | Directory name text color in file trees. |
| `theme::cmd::set` | `String` | Command bar prompt and text color. |
| `theme::add_bg::set` | `String` | Background color for additions in diff views. |
| `theme::del_bg::set` | `String` | Background color for deletions in diff views. |
| `theme::add_fg::set` | `String` | Foreground color for text additions in diff views. |
| `theme::del_fg::set` | `String` | Foreground color for text deletions in diff views. |

### Advanced Highlight Settings
These configure specialized styling behaviours for line changes.

| Property | Type | Description |
| :--- | :--- | :--- |
| `theme::file_staged_highlight::set` | `LineHighlightMode` | Style mode used to highlight staged lines. |
| `theme::file_staged_highlight_opacity::set` | `f64` | Opacity value (ranging `0.0` to `1.0`) for staged line highlights. |
| `theme::file_change_highlight::set` | `LineHighlightMode` | Style mode used to highlight changed lines. |
| `theme::file_change_highlight_opacity::set` | `f64` | Opacity value (ranging `0.0` to `1.0`) for changed line highlights. |

---

## 3. View Actions (`view`)
This namespace contains view-specific operations divided into the `file` viewer and the directory `tree` browser.

### File View Actions (`view::file`)

#### Navigation & Scrolling
* **`view::file::scroll::left(columns: u32)`**
  * Description: Scroll the file contents left by the specified number of columns.
* **`view::file::scroll::right(columns: u32)`**
  * Description: Scroll the file contents right by the specified number of columns.
* **`view::file::cursor::up(lines: u32)`**
  * Description: Move the active cursor up by the specified number of lines.
* **`view::file::cursor::down(lines: u32)`**
  * Description: Move the active cursor down by the specified number of lines.
* **`view::file::cursor::half_page_up()`**
  * Description: Scroll up by half of the terminal page height.
* **`view::file::cursor::half_page_down()`**
  * Description: Scroll down by half of the terminal page height.
* **`view::file::cursor::top()`**
  * Description: Jump directly to the first line of the file.
* **`view::file::cursor::bottom()`**
  * Description: Jump directly to the last line of the file.

#### Diff & Staging Actions
* **`view::file::nav::next_hunk()`**
  * Description: Move focus forward to the next modification hunk.
* **`view::file::nav::prev_hunk()`**
  * Description: Move focus backward to the previous modification hunk.
* **`view::file::staging::toggle()`**
  * Description: Toggle the staging status of the currently focused change.
* **`view::file::staging::toggle_hunk(index: u32)`**
  * Description: Toggle the staging status of a specific hunk by its index.

#### Fold & Viewport Control
* **`view::file::fold::toggle()`**
  * Description: Expand or collapse the code fold at the current cursor location.
* **`view::file::close()`**
  * Description: Exit the active file view and return to the main workspace.

---

### Tree View Actions (`view::tree`)

#### Navigation
* **`view::tree::cursor::up(nodes: u32)`**
  * Description: Move the selection cursor up the tree by the specified number of nodes.
* **`view::tree::cursor::down(nodes: u32)`**
  * Description: Move the selection cursor down the tree by the specified number of nodes.
* **`view::tree::cursor::half_page_up()`**
  * Description: Scroll the file tree up by half a page.
* **`view::tree::cursor::half_page_down()`**
  * Description: Scroll the file tree down by half a page.
* **`view::tree::cursor::top()`**
  * Description: Move focus to the first item in the file tree.
* **`view::tree::cursor::bottom()`**
  * Description: Move focus to the last item in the file tree.

#### Directory Operations
* **`view::tree::directory::expand()`**
  * Description: Expand the selected directory node.
* **`view::tree::directory::collapse()`**
  * Description: Collapse the selected directory node.

#### Staging & Selection
* **`view::tree::staging::toggle_selected()`**
  * Description: Toggle the staging state of the selected file or directory.
* **`view::tree::staging::invert()`**
  * Description: Invert the staging state of all visible nodes in the tree.
* **`view::tree::open_selected()`**
  * Description: Open the currently selected item (e.g., loads a file into the file viewer).
* **`view::tree::open_file(path: String)`**
  * Description: Force-open a specific file view using its system path.
