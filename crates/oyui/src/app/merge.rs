//! Merge confirmation: applies staged selections to the right tree.
use crate::app::events::ExitAction;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState, TreeNode};
use std::error::Error;
use std::path::Path;

/// Applies staged changes to the right directory and returns the exit action.
#[tracing::instrument(skip_all)]
pub fn confirm_and_write(
    tree: &mut FileTree,
    right_dir: &Path,
    cache: &DiffCache,
) -> Result<ExitAction, Box<dyn Error>> {
    apply_tree_changes(&tree.nodes, right_dir, cache, tree.is_file_diff)?;
    Ok(ExitAction::QuitAndMerge)
}

fn apply_tree_changes(
    nodes: &[TreeNode],
    right_dir: &Path,
    cache: &DiffCache,
    is_file_diff: bool,
) -> Result<(), Box<dyn Error>> {
    for node in nodes {
        match node {
            TreeNode::File(f) => {
                if f.state == StagingState::Unstaged {
                    if f.left_path.is_none() {
                        if let Some(r) = &f.right_path {
                            if r.exists() {
                                std::fs::remove_file(r)?;
                            }
                        }
                    } else if let Some(l) = &f.left_path {
                        let r = match &f.right_path {
                            Some(path) => path.clone(),
                            None => {
                                if is_file_diff {
                                    right_dir.to_path_buf()
                                } else {
                                    right_dir.join(&f.path)
                                }
                            }
                        };

                        if let Some(parent) = r.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::copy(l, &r)?;
                    }
                } else if f.state == StagingState::PartiallyStaged {
                    if let Some(crate::diff::DiffResult::Text(diff)) =
                        cache.diffs.get(&f.path).as_deref()
                    {
                        let default_staged = f.state == StagingState::Staged;
                        let out = diff.staged_content(default_staged);

                        let r = match &f.right_path {
                            Some(path) => path.clone(),
                            None => {
                                if is_file_diff {
                                    right_dir.to_path_buf()
                                } else {
                                    right_dir.join(&f.path)
                                }
                            }
                        };

                        if let Some(parent) = r.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&r, out)?;
                    }
                }
            }
            TreeNode::Directory(d) => {
                apply_tree_changes(&d.children, right_dir, cache, is_file_diff)?;
            }
        }
    }
    Ok(())
}
