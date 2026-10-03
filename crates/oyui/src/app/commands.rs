use crate::commons::glob::glob_match;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState};
use crate::view::tree::TreeViewData;
use std::path::PathBuf;

#[tracing::instrument(skip_all, fields(cmd = cmd))]
pub fn execute(cmd: &str, tree: &mut FileTree, tree_view: &mut TreeViewData, cache: &DiffCache) {
    let cmd = cmd.trim();

    if cmd == "invert" || cmd == "i" {
        for node in &mut tree.nodes {
            node.invert_state_recursive();
        }
        return;
    }

    let (verb, pattern) = if let Some(rest) = cmd.strip_prefix("add ").or(cmd.strip_prefix("a ")) {
        (StagingState::Staged, rest)
    } else if let Some(rest) = cmd.strip_prefix("unstage ").or(cmd.strip_prefix("u ")) {
        (StagingState::Unstaged, rest)
    } else {
        return;
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
}

/// Sets the staging state for a file or directory at the given path.
#[tracing::instrument(skip_all)]
pub fn set_state_for_path(tree: &mut FileTree, path: &PathBuf, new_state: StagingState) {
    tree.set_state_for_path(path, new_state);
}
