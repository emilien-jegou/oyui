# Feature tracking

This document is mainly used by developers to track features in progress on the
project. Feature requests are still done via github issues.

## List

- Have the tool work as a difftool for jujutsu
- Have segments view (scroll on whole diff instead of single file)
- advanced search for pattern e.g.:
```
> find: ... # Show all files in view containing match (use rg)
> find changes: ... # Show all files in view containing +/- relative to match
> find in files: ... # interactive search of file in view (use fd)
```
- Allow the tool to be use as a standalone tool (built-in commit and branch navigation)
    - Replace the need for `jj arrange` by adding commit re-ordering
- In-file hunk split (to get back partial files)
- Manage conflict in diff highlight and behavior
- Three-way split
- In split edition e.g. 'd' for deleting a change in file view without leaving the tool
- merge long directory chain e.g. 'packages/ui/src' could be all on one line
- remote merge review with dynamic forge backend (github, gitlab, ...)
    - Open the tool for AI integration to pre-review the changes, give swift summary, and prompt questions
- Identify binary file incoherence of file signature (e.g. an exe with a png extension) tie it to existing integration.
    - Image/video preview


## Accessibility
- Default theme shouldn't change terminal background
- Default theme should check for terminal color support:

| Mode              | Colors                            |
| ----------------- | --------------------------------- |
| Plain ASCII       | None                              |
| ANSI (16-color)   | 16 colors (8 standard + 8 bright) |
| 256-color palette | 256 colors                        |
| 24-bit True Color | ~16.7M colors (2⁸×2⁸×2⁸)          |

- check for nerdfonts/icons support before displaying any.
