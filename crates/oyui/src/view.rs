use crate::ui_state::TreeUiState;
use core_lib::diff_cache::{DiffCache, DiffStats};
use core_lib::tree::{FileTree, StagingState, TreeNode};
use std::path::PathBuf;

/// A flat, render-ready row.
/// Created fresh each frame from the tree structure and current UI state.
#[derive(Debug, Clone)]
pub struct TreeRow {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_folded: bool,
    pub is_last: bool,
    /// A list of booleans representing whether an ancestor at that depth
    /// has more siblings, used to draw the vertical "│" lines.
    pub parent_continuations: Vec<bool>,
    pub staging_state: StagingState,
    pub stats: Option<DiffStats>,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
}

/// Entry point to convert the recursive Tree into a flat list for the TUI widgets.
pub fn build_flat_list(tree: &FileTree, ui_state: &TreeUiState, cache: &DiffCache) -> Vec<TreeRow> {
    let mut rows = Vec::new();

    // First, filter out the nodes that have no modifications or descendants with modifications
    let visible_nodes: Vec<&TreeNode> = tree
        .nodes
        .iter()
        .filter(|node| should_show_node(node, cache))
        .collect();

    let count = visible_nodes.len();
    for (i, node) in visible_nodes.into_iter().enumerate() {
        let is_last = i == count - 1;
        flatten_recursive(node, 0, is_last, &Vec::new(), ui_state, cache, &mut rows);
    }
    rows
}

use ratatui::style::{Color, Modifier, Style};

pub fn get_status_style(status: &str) -> Style {
    match status {
        "A" => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        "M" => Style::default().fg(Color::Yellow),
        "D" => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::Gray),
    }
}

/// The core recursion logic for flattening the tree into a list.
fn flatten_recursive(
    node: &TreeNode,
    depth: usize,
    is_last: bool,
    parent_continuations: &[bool],
    ui_state: &TreeUiState,
    cache: &DiffCache,
    rows: &mut Vec<TreeRow>,
) {
    match node {
        TreeNode::File(file) => {
            // Check stats in the cache (populated by background worker)
            let stats = cache.get_stats(&file.path).value().cloned();

            // Skip files that have been processed and confirmed to have no changes
            if let Some(s) = &stats {
                if s.insertions == 0 && s.deletions == 0 {
                    return;
                }
            }

            rows.push(TreeRow {
                path: file.path.clone(),
                name: file.name.clone(),
                depth,
                is_dir: false,
                is_folded: false,
                is_last,
                parent_continuations: parent_continuations.to_vec(),
                staging_state: file.state,
                stats,
                left_path: file.left_path.clone(),
                right_path: file.right_path.clone(),
            });
        }
        TreeNode::Directory(dir) => {
            let folded = ui_state.is_folded(&dir.path);
            let staging_state = node.compute_staging_state();

            rows.push(TreeRow {
                path: dir.path.clone(),
                name: dir.name.clone(),
                depth,
                is_dir: true,
                is_folded: folded,
                is_last,
                parent_continuations: parent_continuations.to_vec(),
                staging_state,
                stats: None,
                left_path: None,
                right_path: None,
            });

            if !folded {
                // Determine which children are actually visible
                let visible_children: Vec<&TreeNode> = dir
                    .children
                    .iter()
                    .filter(|child| should_show_node(child, cache))
                    .collect();

                // To draw vertical lines for descendants, we need to know if
                // the current folder level has a sibling further down.
                let mut child_continuations = parent_continuations.to_vec();
                child_continuations.push(!is_last);

                let child_count = visible_children.len();
                for (i, child) in visible_children.into_iter().enumerate() {
                    let child_is_last = i == child_count - 1;
                    flatten_recursive(
                        child,
                        depth + 1,
                        child_is_last,
                        &child_continuations,
                        ui_state,
                        cache,
                        rows,
                    );
                }
            }
        }
    }
}

/// Helper to determine if a node should be visible in the TUI.
/// A node is visible if:
/// 1. It is a file with changes (+/-) or its stats are still being computed.
/// 2. It is a directory containing at least one visible descendant.
fn should_show_node(node: &TreeNode, cache: &DiffCache) -> bool {
    match node {
        TreeNode::File(f) => {
            let stats_lazy = cache.get_stats(&f.path);
            if let Some(s) = stats_lazy.value() {
                // If computed, only show if there is actually a diff
                s.insertions > 0 || s.deletions > 0
            } else {
                // If stats are still computing (Unstarted/Started), show the file
                true
            }
        }
        TreeNode::Directory(d) => {
            // Directories are visible if ANY of their children are visible
            d.children
                .iter()
                .any(|child| should_show_node(child, cache))
        }
    }
}
