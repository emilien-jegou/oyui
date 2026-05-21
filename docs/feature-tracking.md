# Feature tracking

This document is mainly used by developers to track features in progress on the
project. Feature requests are still done via github issues.

## List

- Have the tool work as a difftool for jujutsu
- Have next file/prev file navigation for file view
- Have segments view (scroll on whole diff instead of single file)
- advanced search for pattern e.g.:
```
> find: ... # Show all files in view containing match (use rg)
> find changes: ... # Show all files in view containing +/- relative to match
> find in files: ... # interactive search of file in view (use fd)
```
- Allow the tool to be use as a standalone tool (built-in commit and branch navigation)
- In-file hunk split (to get back partial files)
- Manage conflict in diff highlight and behavior
- Three-way split
- In split edition e.g. 'd' for deleting a change in file view without leaving the tool
- merge long directory chain e.g. 'packages/ui/src' could be all on one line
- remote merge review (github)
