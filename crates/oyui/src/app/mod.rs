pub mod commands;
pub mod events;
pub mod merge;
pub mod worker;

pub use events::{CommandMode, ExitAction};

use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, TreeNode};
use crate::view::View;
use crate::worker::Tasker;
use std::path::PathBuf;
use std::sync::Arc;
use syntect::highlighting::Theme;

pub struct App {
    pub tree: FileTree,
    pub cache: DiffCache,
    pub view: View,
    pub theme: UiTheme,
    pub syntax_theme: Arc<Theme>,
    pub config_error: Option<String>,

    pub command_mode: CommandMode,
    pub should_quit: bool,

    pub worker: Tasker,

    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub base_path: Option<PathBuf>,
}

impl App {
    pub fn new(worker: Tasker) -> Self {
        let (theme, syntax_theme) = crate::config::builtin::fallback_theme();

        Self {
            tree: FileTree::new(),
            cache: DiffCache::default(),
            view: View::new(),
            theme,
            syntax_theme: Arc::new(syntax_theme),
            config_error: None,
            command_mode: CommandMode::Normal,
            should_quit: false,
            worker,

            left_path: None,
            right_path: None,
            base_path: None,
        }
    }

    pub async fn tick(&mut self) {
        // Pass the syntax_theme reference directly to the event processor
        worker::process_events(&mut self.worker, &mut self.cache, &self.syntax_theme).await;
    }

    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&mut self) {
        let _ = self.worker.shutdown().await;
    }

    #[tracing::instrument(skip_all, fields(cmd = cmd))]
    pub fn execute_command(&mut self, cmd: &str) {
        commands::execute(cmd, &mut self.tree, &self.view.tree_view, &self.cache);
    }

    /// Synchronizes the cache's line_selections with the structural FileTree state.
    /// This resolves desyncs when a file is staged/unstaged from the Sidebar.
    pub fn sync_cache_with_tree(&mut self) {
        Self::sync_cache_recursive(&self.tree.nodes, &mut self.cache);
    }

    fn sync_cache_recursive(nodes: &[TreeNode], cache: &mut DiffCache) {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.state == crate::tree::StagingState::Staged
                        || f.state == crate::tree::StagingState::Unstaged
                    {
                        let target_val = f.state == crate::tree::StagingState::Staged;

                        let mut diff_clone = None;
                        if let Some(val) = cache.diffs.get(&f.path).value() {
                            if let crate::diff::DiffResult::Text(diff) = val {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                // Only clone and rewrite if there's actually a desync
                                let needs_sync = diff.line_selections.len() != total_lines
                                    || diff.line_selections.iter().any(|&v| v != target_val);

                                if needs_sync {
                                    diff_clone = Some(val.clone());
                                }
                            }
                        }

                        if let Some(mut diff_result) = diff_clone {
                            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                diff.line_selections.clear();
                                diff.line_selections.resize(total_lines, target_val);
                            }
                            cache.diffs.set(f.path.clone(), diff_result);
                        }
                    }
                }
                TreeNode::Directory(d) => {
                    Self::sync_cache_recursive(&d.children, cache);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn toggle_stage_hunk(&mut self, hunk_idx: usize) {
        let Some(path) = self.view.file_view.current_path.clone() else {
            return;
        };

        let mut diff_clone = None;
        if let Some(val) = self.cache.diffs.get(&path).value() {
            diff_clone = Some(val.clone());
        }

        if let Some(mut diff_result) = diff_clone {
            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();

                // Inherit the overall file status to apply to uninitialized lines
                let default_staged = self
                    .tree
                    .get_file_state(&path)
                    .unwrap_or(crate::tree::StagingState::Unstaged)
                    == crate::tree::StagingState::Staged;

                if diff.line_selections.len() != total_lines {
                    diff.line_selections.resize(total_lines, default_staged);
                }

                let mut start_idx = 0;
                for hunk in diff.hunks.iter().take(hunk_idx) {
                    start_idx += hunk.lines.len();
                }

                if let Some(hunk) = diff.hunks.get(hunk_idx) {
                    let mut all_staged = true;
                    for (j, line) in hunk.lines.iter().enumerate() {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        )
                            && !diff
                                .line_selections
                                .get(start_idx + j)
                                .copied()
                                .unwrap_or(default_staged)
                            {
                                all_staged = false;
                                break;
                            }
                    }

                    let new_state = !all_staged;
                    for (j, line) in hunk.lines.iter().enumerate() {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        )
                            && start_idx + j < diff.line_selections.len() {
                                diff.line_selections[start_idx + j] = new_state;
                            }
                    }
                }

                let mut has_staged = false;
                let mut has_unstaged = false;
                let mut current_idx = 0;
                for h in &diff.hunks {
                    for line in &h.lines {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        ) {
                            let is_staged = diff
                                .line_selections
                                .get(current_idx)
                                .copied()
                                .unwrap_or(default_staged);
                            if is_staged {
                                has_staged = true;
                            } else {
                                has_unstaged = true;
                            }
                        }
                        current_idx += 1;
                    }
                }

                let new_staging_state = if has_staged && has_unstaged {
                    crate::tree::StagingState::PartiallyStaged
                } else if has_staged {
                    crate::tree::StagingState::Staged
                } else {
                    crate::tree::StagingState::Unstaged
                };

                self.update_file_state(&path, new_staging_state);
            }

            self.cache.diffs.set(path, diff_result);
        }
    }

    fn update_file_state(&mut self, path: &PathBuf, new_state: crate::tree::StagingState) {
        fn find_and_update(
            nodes: &mut [TreeNode],
            path: &PathBuf,
            new_state: crate::tree::StagingState,
        ) -> bool {
            for node in nodes {
                match node {
                    TreeNode::File(f) => {
                        if f.path == *path {
                            f.state = new_state;
                            return true;
                        }
                    }
                    TreeNode::Directory(d) => {
                        if find_and_update(&mut d.children, path, new_state) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        find_and_update(&mut self.tree.nodes, path, new_state);
    }

    #[tracing::instrument(skip_all)]
    pub fn confirm_merge(&mut self) -> Result<ExitAction, Box<dyn std::error::Error>> {
        let right_dir = self.right_path.clone().ok_or("Right path not set")?;
        merge::confirm_and_write(
            &mut self.tree,
            &mut self.should_quit,
            &right_dir,
            &self.cache,
        )
    }

    pub fn get_diff_summary(&self) -> (usize, usize, usize) {
        let (mut a, mut d, mut m) = (0, 0, 0);
        self.count_recursive(&self.tree.nodes, &mut a, &mut d, &mut m);
        (a, d, m)
    }

    fn count_recursive(&self, nodes: &[TreeNode], a: &mut usize, d: &mut usize, m: &mut usize) {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.left_path.is_none() {
                        *a += 1;
                    } else if f.right_path.is_none() {
                        *d += 1;
                    } else {
                        *m += 1;
                    }
                }
                TreeNode::Directory(dir) => self.count_recursive(&dir.children, a, d, m),
            }
        }
    }
}
