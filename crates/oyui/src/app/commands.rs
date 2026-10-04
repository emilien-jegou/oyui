use crate::commons::glob::glob_match;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState};
use crate::view::tree::TreeViewData;
use std::path::PathBuf;

/// Runs a built-in palette command; returns whether it recognized `cmd`.
#[tracing::instrument(skip_all, fields(cmd = cmd))]
pub fn execute(
    cmd: &str,
    tree: &mut FileTree,
    tree_view: &mut TreeViewData,
    cache: &DiffCache,
) -> bool {
    let cmd = cmd.trim();

    if cmd == "invert" || cmd == "i" {
        for node in &mut tree.nodes {
            node.invert_state_recursive();
        }
        return true;
    }

    let (verb, pattern) = if let Some(rest) = cmd.strip_prefix("add ").or(cmd.strip_prefix("a ")) {
        (StagingState::Staged, rest)
    } else if let Some(rest) = cmd.strip_prefix("unstage ").or(cmd.strip_prefix("u ")) {
        (StagingState::Unstaged, rest)
    } else {
        return false;
    };

    let rows = tree_view.flat_rows(tree, cache);
    let matching: Vec<PathBuf> = rows
        .iter()
        .filter(|r| !r.is_dir && glob_match(pattern, &r.path))
        .map(|r| r.path.clone())
        .collect();

    for path in matching {
        set_state_for_path(tree, &path, verb);
    }
    true
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

        assert!(
            !execute("bogus", &mut tree, &mut tree_view, &cache),
            "unknown commands must fall through to script commands"
        );
        assert!(
            execute("invert", &mut tree, &mut tree_view, &cache),
            "built-in commands must report as handled"
        );
    }
}
