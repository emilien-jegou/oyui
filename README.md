# Oyui

**Oyui** is a TUI merge tool and staging interface designed for [Jujutsu](https://github.com/martinvonz/jj) and Git.

![Redesign screenshot](./docs/assets/screen0.png)

## Why Another merge editor?

While Jujutsu is a powerful VCS, I found the built-in diff-editing experience (via `scm-record`) to be limited. It lacked syntax highlighting, was mostly monochromatic, and made it difficult to visualize the impact of changes across full files. Although some more polished solution like `lightjj` exist (web based), we were missing a modern TUI merge editor.

I built **Oyui** to bridge this gap, focusing on a friction-free experience with:
*   **Intuitive:** One view, natural navigation, simple UX.
*   **Visual Clarity:** Balanced, and color-coded tree-view that is easy on the eyes.
*   **Keyboard-First:** Simple keybinds, hjkl (following vim tradition) or arrow key.
*   **Efficient Workflow:** Perform bulk actions (like staging by folder) instantly via the command palette. see feature section.

## Features

*   **Command Palette:** Perform bulk operations with simple commands.
    *   `:add ./icons/*` (or `:a`) to stage files.
    *   `:unstage ./icons/*` (or `:u`) to unstage files.
*   **Binary support:** Infer binary files format using their [magic number signature](https://en.wikipedia.org/wiki/Magic_number_(programming)).
*   **Theming:** 30+ themes, check [full list](./docs/themes.md).
*   **Live config reload:** modify the config, see changes live.
*   **Partial staging:** stage only changes you need.
*   **Themed Diffs:** Beautiful, readable syntax highlighting for your changes.
    ![Redesign screenshot](./docs/assets/screen1.png)

---

## Roadmap & Feedback
Follow the progress of new features on the [Feature Tracking page](./docs/feature-tracking.md). Have an idea? [Open an issue](https://github.com/emilien-jegou/oyui/issues/new)!

---

## Installation (Nix Flakes)

Add `oyui` to your `flake.nix` inputs:

```nix
inputs.oyui = {
  url = "github:emilien-jegou/oyui";
  inputs.nixpkgs.follows = "nixpkgs"; 
};
```

Then, add it to your system packages:

```nix
environment.systemPackages = [
  inputs.oyui.packages.${pkgs.system}.default
];
```

---

## Configuration

### Usage for Jujutsu (`config.toml`)
Configure Jujutsu to use Oyui as your primary diff editor:

```toml
[ui]
diff-editor = "oyui"
diff-instructions = false

[merge-tools.oyui]
program = "oyui"
edit-args = ["-d", "$left", "$right"]
```

### ~/.config/oyui/config.toml

Setup the default config for oyui
```toml
# Oyui comes with 30+ built-in themes out of the box:
# aura, ayu, catppuccin-mocha, dracula, gruvbox-dark, nord,
# one-dark, everforest-light...
#
# Full list at:
# https://github.com/emilien-jegou/oyui/tree/main/docs/themes
chosen_theme = "catppuccin-mocha"

[theme.my-custom-theme]
# (Optional) Path to a TextMate file.
tm_theme_path = "~/.config/oyui/themes/MyCustomSyntax.tmTheme"

# Define the UI colors for this theme.
# Values can be standard Hex codes ("#RRGGBB") or references to the 
# loaded tmTheme syntax file using the "tm:" prefix.
[theme.my-custom-theme.colors]

# -- General UI --
bg = "tm:background"        # UI Background
cursor_bg = "#1e1e2a"       # Selection/Cursor row background
fg = "tm:foreground"        # Primary text color
dim = "tm:gutter_foreground" # Secondary text (hints, tree lines)

# -- File Tree & Status --
staged = "tm:string"        # Color for staged changes
unstaged = "tm:comment"     # Color for unstaged changes
partial = "tm:constant"     # Color for partials
dir = "tm:entity"           # Color for directories

# -- Command Bar --
cmd = "tm:accent"           # Color for command prompt

# -- Diff View --
add_bg = "#1e2d1e"          # Background for additions
del_bg = "#2d1e1e"          # Background for deletions
add_fg = "tm:markup.inserted" # Text color for additions
del_fg = "tm:markup.deleted"  # Text color for deletions

# ==========================================
# Available "tm:" references
# ==========================================
# If `tm_theme_path` is set, you can map these to your UI:
#
# tm:foreground, tm:background, tm:caret, tm:line_highlight, tm:misspelling, 
# tm:minimap_border, tm:accent, tm:bracket_contents_foreground, 
# tm:brackets_foreground, tm:brackets_background, tm:tags_foreground, 
# tm:highlight, tm:find_highlight, tm:find_highlight_foreground, tm:gutter, 
# tm:gutter_foreground, tm:selection, tm:selection_foreground, 
# tm:selection_border, tm:inactive_selection, tm:inactive_selection_foreground, 
# tm:guide, tm:active_guide, tm:stack_guide, tm:shadow
```
---

## Credits
*   [scm-record](https://github.com/arxanas/scm-record)
*   [oyo](https://github.com/ahkohd/oyo)
