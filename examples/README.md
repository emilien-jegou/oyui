# Examples

Two kinds of example live here:

- **Configurations** — runnable `.rn` files (see the table below).
- **Scenario fixtures** — data under `diff/`, `merge/`, `conflict/` driven by
  [`example.sh`](./example.sh).

## Running scenarios

`example.sh` is the single entrypoint. It drives the **local** binary (built
from this checkout) and copies fixtures into a scratch directory so writes
never touch the repository.

```sh
examples/example.sh list          # list scenarios
examples/example.sh help          # usage
examples/example.sh merge         # three-way merge with a conflict
examples/example.sh git-mergetool # resolve a real git conflict with oyui
examples/example.sh check         # verify fixtures + run the test suite
examples/example.sh clean
```

| Scenario | What it does |
|---|---|
| `diff` | Read-only two-way directory diff. |
| `staged` | Stage a two-way diff; the result is written back to the copy. |
| `merge` | Synthesized three-way merge with a conflicting region. |
| `merge-clean` | Synthesized three-way merge with no conflicts. |
| `conflict` | Resolve a target that already contains conflict markers. |
| `git-difftool` | Run oyui through `git difftool` on a scratch repo. |
| `git-mergetool` | Run oyui through `git mergetool` on a scratch repo. |
| `jj-diffedit` / `jj-resolve` | Same via `jj` (needs `jj` installed). |
| `check` | Validate fixtures and run `cargo test -p oyui`. |
| `clean` | Remove the scratch directory. |

Environment: `OYUI_BIN` to pick a binary, `OYUI_EXAMPLES_DIR` for the scratch
dir (default `/tmp/oyui-examples`), `OYUI_EXAMPLE_DRY=1` to print the command
instead of launching the TUI.

## Configurations

Runnable `.rn` configuration snippets. Each file is a complete
`config()` you can copy to `~/.config/oyui/config.rn` or load with
`oyui -c examples/<file>`.

Every example is compiled by the test suite
(`script::tests::bundled_examples_compile`), so they stay valid.

| File | Shows |
|---|---|
| [`01-theme.rn`](./01-theme.rn) | Theme selection, color overrides, gradients, glyphs. |
| [`02-keybindings.rn`](./02-keybindings.rn) | Rebinding, `on_mode`, multi-modifier chords. |
| [`03-navigation.rn`](./03-navigation.rn) | Cursor/hunk movement, folding, opening files. |
| [`04-staging.rn`](./04-staging.rn) | Hunk/line staging, bulk staging, undo/redo. |
| [`05-copy-hunk.rn`](./05-copy-hunk.rn) | Clipboard via `global::copy`, hunk inspection. |
| [`06-commit-regex.rn`](./06-commit-regex.rn) | Regex-scan files and stage matches (async). |
| [`07-custom-commands.rn`](./07-custom-commands.rn) | Registering palette commands. |
| [`08-events.rn`](./08-events.rn) | Reacting to `file_opened`, `config_reload`, etc. |
| [`09-ui.rn`](./09-ui.rn) | Hint bar, tree rows, status line, settings. |
| [`10-analysis.rn`](./10-analysis.rn) | Diff introspection and content analysis. |
| [`11-unbind.rn`](./11-unbind.rn) | Removing built-in binds selectively or wholesale. |
| [`12-help.rn`](./12-help.rn) | Named keybinds and the `?` help overlay. |

See [../docs/actions.md](../docs/actions.md) for the full API and
[../docs/configuration.md](../docs/configuration.md) for the workflow.
