use crate::app::events::ExitAction;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, StagingState, TreeNode};
use std::error::Error;
use std::path::Path;

#[tracing::instrument(skip_all)]
pub fn confirm_and_write(
    tree: &mut FileTree,
    should_quit: &mut bool,
    right_dir: &Path,
    cache: &DiffCache,
) -> Result<ExitAction, Box<dyn Error>> {
    apply_tree_changes(&tree.nodes, right_dir, cache, tree.is_file_diff)?;
    *should_quit = true;
    Ok(ExitAction::KeepRunning)
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
                        cache.diffs.get(&f.path).value()
                    {
                        let mut out = String::new();
                        let old_lines: Vec<&str> = diff.old_file_content.split('\n').collect();
                        let new_lines: Vec<&str> = diff.new_file_content.split('\n').collect();

                        let mut current_old_line = 0;
                        let mut selection_idx = 0;
                        let mut first_line_written = false;

                        for hunk in &diff.hunks {
                            while current_old_line < hunk.before_lines.start {
                                if current_old_line < old_lines.len() {
                                    if first_line_written {
                                        out.push('\n');
                                    }
                                    out.push_str(old_lines[current_old_line]);
                                    first_line_written = true;
                                }
                                current_old_line += 1;
                            }

                            for diff_line in &hunk.lines {
                                let is_staged =
                                    *diff.line_selections.get(selection_idx).unwrap_or(&false);
                                selection_idx += 1;

                                match diff_line {
                                    crate::diff::DiffLine::Context { old_line_idx, .. } => {
                                        if *old_line_idx < old_lines.len() {
                                            if first_line_written {
                                                out.push('\n');
                                            }
                                            out.push_str(old_lines[*old_line_idx]);
                                            first_line_written = true;
                                        }
                                        current_old_line = *old_line_idx + 1;
                                    }
                                    crate::diff::DiffLine::Deletion { old_line_idx, .. } => {
                                        if !is_staged
                                            && *old_line_idx < old_lines.len() {
                                                if first_line_written {
                                                    out.push('\n');
                                                }
                                                out.push_str(old_lines[*old_line_idx]);
                                                first_line_written = true;
                                            }
                                        current_old_line = *old_line_idx + 1;
                                    }
                                    crate::diff::DiffLine::Addition { new_line_idx, .. } => {
                                        if is_staged
                                            && *new_line_idx < new_lines.len() {
                                                if first_line_written {
                                                    out.push('\n');
                                                }
                                                out.push_str(new_lines[*new_line_idx]);
                                                first_line_written = true;
                                            }
                                    }
                                }
                            }
                        }

                        // Output any remaining unchanged old lines
                        while current_old_line < old_lines.len() {
                            if first_line_written {
                                out.push('\n');
                            }
                            out.push_str(old_lines[current_old_line]);
                            first_line_written = true;
                            current_old_line += 1;
                        }

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
