# Examples

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
