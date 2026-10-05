# Default keybindings

These are the built-in bindings. Override them with `keybind`, remove one with
`unbind`, or clear/restore them all with `unbind_all` / `reset_keybinds` (see
[configuration.md](./configuration.md#keybinds)).

## Global

Active in every view.

| Key | Action |
|---|---|
| `enter` | Confirm / open the merge window |
| `q`, `ctrl-c` | Quit |
| `:` | Open the command prompt |
| `?` | Show all keybindings (help overlay) |
| `u` | Undo |
| `ctrl-r` | Redo |
| `h`, `esc` | Close the file view |

## File view

| Key | Action |
|---|---|
| `j`, `down` | Cursor down |
| `k`, `up` | Cursor up |
| `ctrl-d` | Half page down |
| `ctrl-u` | Half page up |
| `ctrl-f`, `page down` | Page down |
| `ctrl-b`, `page up` | Page up |
| `g` | Top |
| `G` | Bottom |
| `n` | Next hunk |
| `N` | Previous hunk |
| `ctrl-l`, `right` | Scroll right |
| `ctrl-h`, `left` | Scroll left |
| `space` | Toggle hunk staging at the cursor |
| `t` | Toggle line staging |
| `s` | Split hunk at the cursor |
| `i` | Invert selections |
| `z` | Toggle unchanged-context folding |
| `o` | Fold the conflict at the cursor (ours); press again to expand |
| `T` | Fold the conflict at the cursor (theirs) |
| `B` | Fold the conflict at the cursor (both) |

## Tree view

| Key | Action |
|---|---|
| `j`, `down` | Cursor down |
| `k`, `up` | Cursor up |
| `ctrl-d` | Down 20 rows |
| `ctrl-u` | Up 20 rows |
| `ctrl-f`, `page down` | Page down |
| `ctrl-b`, `page up` | Page up |
| `g` | Top |
| `G` | Bottom |
| `l`, `right` | Open the selection |
| `h`, `left` | Collapse the selection |
| `space` | Toggle staging for the selection |
| `i` | Invert all staging |

> Note: `h`/`esc` are also globally bound to "close the file view". In the tree
> view, `h` therefore both collapses the selection and clears the open-file
> path. Neither effect is visible in the tree view, but it means `h` is doing
> two jobs. Rebind one of them if this matters to you.
