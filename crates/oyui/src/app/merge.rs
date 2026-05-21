use crate::app::events::ExitAction;
use crate::tree::{FileTree, StagingState, TreeNode};
use std::error::Error;

#[tracing::instrument(skip_all)]
pub fn confirm_and_write(
    tree: &mut FileTree,
    should_quit: &mut bool,
) -> Result<ExitAction, Box<dyn Error>> {
    let has_staged_changes = is_anything_staged(&tree.nodes);

    if !has_staged_changes {
        return Ok(ExitAction::QuitWithReason(
            "No changes staged. Aborting merge.".into(),
        ));
    }

    for node in &mut tree.nodes {
        node.invert_state_recursive();
    }

    apply_tree_changes(&tree.nodes)?;
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

fn apply_tree_changes(nodes: &[TreeNode]) -> Result<(), Box<dyn Error>> {
    for node in nodes {
        match node {
            TreeNode::File(f) => {
                if f.state == StagingState::Staged {
                    if let (Some(l), Some(r)) = (&f.left_path, &f.right_path) {
                        std::fs::copy(l, r)?;
                    }
                }
            }
            TreeNode::Directory(d) => {
                apply_tree_changes(&d.children)?;
            }
        }
    }
    Ok(())
}
