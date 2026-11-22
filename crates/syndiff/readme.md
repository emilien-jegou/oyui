# oyui-syndiff

Re-export of the **syndiff** crate for oyui.

[Check the official repository for more information](https://github.com/emilien-jegou/oyui)

## Using the library

The crate exposes both low-level tree-based operations (`build_tree`,
`diff_trees`) and higher-level source/line operations (`diff_source`,
`diff_source_lines`, `diff_line_ops`, and `diff_inline`). See the crate-level
API documentation for runnable examples and the semantics of returned byte
ranges and line operations.

```toml
[dependencies]
oyui-syndiff = "0.0.7"
```

The default `all-languages` feature enables parsers for Rust, Python, Go,
JavaScript, TypeScript/TSX, C, C++, C#, Java, Ruby, PHP, JSON, YAML, TOML, HTML,
CSS, and Bash. To reduce compile time and dependency size, disable defaults and
enable only the grammars you need:

```toml
[dependencies]
oyui-syndiff = { version = "0.0.7", default-features = false, features = ["rust", "json"] }
```

The `diff_source*` entry points select a parser using the file extension and
return `FailedDiff` for unsupported/missing extensions, parse problems, or
graph limits. Structural diffing is not guaranteed to work well on heavily
broken syntax; callers can fall back to a conventional line diff for
`TooManyErrors` or `GraphLimitExceeded`. The low-level `diff_trees` API instead
returns `None` when its graph limit is exceeded.

### ✦ Philosophy

syndiff is a lightweight, standalone library that implements a structural diff algorithm inspired by **Difftastic**, designed to be embedded inside other tools and applications.


Traditional diffs treat code as text. Syndiff treats code as having *shape*.

This library exists to make syntax-aware diffing easy to integrate into editors, code review tools, language servers, and research projects - without dragging along a full application.

It does not aim to replace Difftastic. It distills its core ideas into a reusable form.

### ✦ Inspiration

Inspired by **[Difftastic](https://github.com/Wilfred/difftastic)**, created by Wilfred Hughes. All credit for the original algorithmic ideas belongs there. Any bugs introduced here are new and entirely our own.
