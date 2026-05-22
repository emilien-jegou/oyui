use crate::app::events::ExitAction;
use crate::tree::{FileTree, StagingState, TreeNode};
use std::error::Error;
use std::path::Path;

#[tracing::instrument(skip_all)]
pub fn confirm_and_write(
    tree: &mut FileTree,
    should_quit: &mut bool,
    right_dir: &Path,
) -> Result<ExitAction, Box<dyn Error>> {
    let has_staged_changes = is_anything_staged(&tree.nodes);

    if !has_staged_changes {
        return Ok(ExitAction::QuitWithReason(
            "No changes staged. Aborting merge.".into(),
        ));
    }

    apply_tree_changes(&tree.nodes, right_dir)?;
    *should_quit = true;
    Ok(ExitAction::KeepRunning)
}

fn is_anything_staged(nodes: &[TreeNode]) -> bool {
    for node in nodes {
        match node {
            TreeNode::File(f) => {
                if f.state == StagingState::Staged {
                    return true;
                }
            }
            TreeNode::Directory(d) => {
                if is_anything_staged(&d.children) {
                    return true;
                }
            }
        }
    }
    false
}

fn apply_tree_changes(nodes: &[TreeNode], right_dir: &Path) -> Result<(), Box<dyn Error>> {
    for node in nodes {
        match node {
            TreeNode::File(f) => {
                // In a split workflow, the Right directory is the output and is pre-populated
                // with the child commit. We must REVERT unstaged changes back to the Left (parent) state.
                if f.state == StagingState::Unstaged {
                    if f.left_path.is_none() {
                        // Added file: exists in Right, but not in Left. Unstaged means we revert the addition (delete it).
                        if let Some(r) = &f.right_path {
                            if r.exists() {
                                std::fs::remove_file(r)?;
                            }
                        }
                    } else if let Some(l) = &f.left_path {
                        // Modified or Deleted file. We restore the Left content into the Right directory.
                        let r = match &f.right_path {
                            Some(path) => path.clone(),
                            None => right_dir.join(&f.path),
                        };

                        // Ensure parent directories exist (crucial if restoring a file inside a deleted subfolder)
                        if let Some(parent) = r.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::copy(l, &r)?;
                    }
                }
            }
            TreeNode::Directory(d) => {
                apply_tree_changes(&d.children, right_dir)?;
            }
        }
    }
    Ok(())
}
