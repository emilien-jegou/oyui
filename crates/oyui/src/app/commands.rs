use crate::commons::glob::glob_match;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState};
use crate::view::tree::TreeViewData;
use std::path::PathBuf;

/// Runs a built-in palette command.
///
/// Returns the number of files the command applied to when `cmd` is a
/// built-in — including `Some(0)` when a pattern matched nothing, so the
/// caller can tell a typo from success — and `None` when `cmd` is not a
/// built-in and should fall through to script commands.
#[tracing::instrument(skip_all, fields(cmd = cmd))]
pub fn execute(
    cmd: &str,
    tree: &mut FileTree,
    tree_view: &mut TreeViewData,
    cache: &DiffCache,
) -> Option<usize> {
    let cmd = cmd.trim();

    if cmd == "invert" || cmd == "i" {
        for node in &mut tree.nodes {
            node.invert_state_recursive();
        }
        return Some(tree.files().count());
    }

    let (verb, pattern) = if let Some(rest) = cmd.strip_prefix("add ").or(cmd.strip_prefix("a ")) {
        (StagingState::Staged, rest)
    } else if let Some(rest) = cmd.strip_prefix("unstage ").or(cmd.strip_prefix("u ")) {
        (StagingState::Unstaged, rest)
    } else {
        return None;
    };

    let rows = tree_view.flat_rows(tree, cache);
    let matching: Vec<PathBuf> = rows
        .iter()
        .filter(|r| !r.is_dir && glob_match(pattern, &r.path))
        .map(|r| r.path.clone())
        .collect();

    for path in &matching {
        set_state_for_path(tree, path, verb);
    }
    Some(matching.len())
}

/// Sets the staging state for a file or directory at the given path.
#[tracing::instrument(skip_all)]
pub fn set_state_for_path(tree: &mut FileTree, path: &PathBuf, new_state: StagingState) {
    tree.set_state_for_path(path, new_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_cache::DiffCache;

    #[test]
    fn execute_reports_whether_it_handled_the_command() {
        let mut tree = FileTree::default();
        let mut tree_view = TreeViewData::default();
        let cache = DiffCache::default();

        assert_eq!(
            execute("bogus", &mut tree, &mut tree_view, &cache),
            None,
            "unknown commands must fall through to script commands"
        );
        assert_eq!(
            execute("invert", &mut tree, &mut tree_view, &cache),
            Some(0),
            "built-in commands must report as handled"
        );
    }

    #[test]
    fn pattern_commands_report_their_match_count() {
        let mut tree = FileTree::default();
        tree.insert_file(
            PathBuf::from("src/a.rs"),
            None,
            Some(PathBuf::from("right/a.rs")),
        );
        tree.insert_file(
            PathBuf::from("src/b.rs"),
            None,
            Some(PathBuf::from("right/b.rs")),
        );
        let mut tree_view = TreeViewData::default();
        let cache = DiffCache::default();

        assert_eq!(
            execute("add src/*.rs", &mut tree, &mut tree_view, &cache),
            Some(2),
            "the count of matched files is the applied count"
        );
        assert_eq!(
            execute("add nosuch/*.rs", &mut tree, &mut tree_view, &cache),
            Some(0),
            "a pattern matching nothing reports zero, not success"
        );
    }
}
