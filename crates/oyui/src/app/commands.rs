use crate::commons::glob::glob_match;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState, TreeNode};
use crate::view::tree::TreeViewData;
use std::path::PathBuf;

#[tracing::instrument(skip_all, fields(cmd = cmd))]
pub fn execute(cmd: &str, tree: &mut FileTree, tree_view: &TreeViewData, cache: &DiffCache) {
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

#[tracing::instrument(skip_all)]
pub fn set_state_for_path(tree: &mut FileTree, path: &PathBuf, new_state: StagingState) {
    for node in &mut tree.nodes {
        if apply_state_recursive(node, path, new_state) {
            break;
        }
    }
}

fn apply_state_recursive(node: &mut TreeNode, target: &PathBuf, new_state: StagingState) -> bool {
    match node {
        TreeNode::File(f) => {
            if &f.path == target {
                f.state = new_state;
                return true;
            }
        }
        TreeNode::Directory(dir) => {
            if &dir.path == target {
                for child in &mut dir.children {
                    child.set_state_recursive(new_state);
                }
                return true;
            }
            for child in &mut dir.children {
                if apply_state_recursive(child, target, new_state) {
                    return true;
                }
            }
        }
    }
    false
}
