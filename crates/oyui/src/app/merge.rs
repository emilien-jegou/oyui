//! Merge confirmation: applies staged selections to the write target.
use crate::app::events::ExitAction;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState, TreeNode};
use std::error::Error;
use std::path::Path;

/// Applies staged changes to `target` and returns the exit action.
///
/// For directory diffs `target` is the destination root; for single-file
/// sessions (`is_file_diff`) it is the destination file itself.
#[tracing::instrument(skip_all)]
pub fn confirm_and_write(
    tree: &mut FileTree,
    target: &Path,
    cache: &DiffCache,
) -> Result<ExitAction, Box<dyn Error>> {
    apply_tree_changes(&tree.nodes, target, cache, tree.is_file_diff)?;
    Ok(ExitAction::QuitAndMerge)
}

fn apply_tree_changes(
    nodes: &[TreeNode],
    target: &Path,
    cache: &DiffCache,
    is_file_diff: bool,
) -> Result<(), Box<dyn Error>> {
    for node in nodes {
        match node {
            TreeNode::File(f) => {
                let dest = if is_file_diff {
                    target.to_path_buf()
                } else {
                    target.join(&f.path)
                };

                match f.state {
                    StagingState::Staged => {}
                    StagingState::Unstaged => {
                        if f.left_path.is_none() {
                            // Revert an addition by removing the destination.
                            if dest.exists() {
                                std::fs::remove_file(&dest)?;
                            }
                        } else if let Some(left) = &f.left_path {
                            // Revert a modification/deletion by restoring the left side.
                            if let Some(parent) = dest.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::copy(left, &dest)?;
                        }
                    }
                    StagingState::PartiallyStaged => {
                        if let Some(crate::diff::DiffResult::Text(diff)) =
                            cache.diffs.get(&f.path).as_deref()
                        {
                            // Selections already describe what to keep, so the
                            // staged content is written directly.
                            let out = diff.staged_content(false);
                            if let Some(parent) = dest.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::write(&dest, out)?;
                        }
                    }
                }
            }
            TreeNode::Directory(d) => {
                apply_tree_changes(&d.children, target, cache, is_file_diff)?;
            }
        }
    }
    Ok(())
}
