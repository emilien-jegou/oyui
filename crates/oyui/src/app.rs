use crate::ui_state::TreeUiState;
use crate::view::{build_flat_list, TreeRow};
use core_lib::diff_cache::DiffCache;
use core_lib::tree::{FileTree, StagingState, TreeNode};
use core_lib::worker::{AsyncWorkerEvent, WorkerRequest};
use crossbeam_channel::{Receiver, Sender};
use ratatui::widgets::ListState;
use std::error::Error;
use std::path::PathBuf;

pub enum ViewMode {
    Tree,
    FileView(PathBuf),
}

pub enum CommandMode {
    Normal,
    /// The string is the current command buffer, e.g. "add ./src"
    Active(String),
    ConfirmMerge,
}

#[derive(PartialEq, Eq)]
pub enum ExitAction {
    KeepRunning,
    QuitAndMerge,
    QuitWithAbort,
    QuitWithReason(String),
}

pub struct App {
    pub tree: FileTree,
    pub cache: DiffCache,
    pub ui_state: TreeUiState,

    pub view_mode: ViewMode,
    pub command_mode: CommandMode,
    pub selected_index: usize,
    pub should_quit: bool,

    pub worker_tx: Sender<WorkerRequest>,
    pub worker_rx: Receiver<AsyncWorkerEvent>,

    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub base_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,

    pub file_scroll_state: ListState,
}

impl App {
    pub fn new(worker_tx: Sender<WorkerRequest>, worker_rx: Receiver<AsyncWorkerEvent>) -> Self {
        Self {
            tree: FileTree::new(),
            cache: DiffCache::new(),
            ui_state: TreeUiState::new(),
            view_mode: ViewMode::Tree,
            command_mode: CommandMode::Normal,
            selected_index: 0,
            should_quit: false,
            worker_tx,
            worker_rx,

            left_path: None,
            right_path: None,
            base_path: None,
            output_path: None,

            file_scroll_state: ListState::default(),
        }
    }

    pub fn confirm_and_write_merge(&mut self) -> Result<ExitAction, Box<dyn Error>> {
        let has_staged_changes = self.is_anything_staged(&self.tree.nodes);

        if !has_staged_changes {
            return Ok(ExitAction::QuitWithReason(
                "No changes staged. Aborting merge.".into(),
            ));
        }

        self.apply_tree_changes(&self.tree.nodes.clone())?;
        self.should_quit = true;
        Ok(ExitAction::KeepRunning)
    }

    fn is_anything_staged(&self, nodes: &[TreeNode]) -> bool {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.state == StagingState::Staged {
                        return true;
                    }
                }
                TreeNode::Directory(d) => {
                    if self.is_anything_staged(&d.children) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn apply_tree_changes(&self, nodes: &[TreeNode]) -> Result<(), Box<dyn Error>> {
        use core_lib::tree::TreeNode;
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.state == StagingState::Staged {
                        // Overwrite the 'right' (new) file with the 'left' (local) version
                        if let (Some(l), Some(r)) = (&f.left_path, &f.right_path) {
                            std::fs::copy(l, r)?;
                        }
                    }
                }
                TreeNode::Directory(d) => {
                    self.apply_tree_changes(&d.children)?;
                }
            }
        }
        Ok(())
    }

    pub fn tick(&mut self) {
        while let Ok(event) = self.worker_rx.try_recv() {
            match event {
                AsyncWorkerEvent::DiffStatsReady(path, stats) => {
                    self.cache.set_stats(path, stats);
                }
                AsyncWorkerEvent::FullDiffReady(path, diff) => {
                    self.cache.set_diff(path, diff);
                }
            }
        }
    }

    /// Flat list rebuilt each frame — cheap since it's just pointer copies
    pub fn flat_rows(&self) -> Vec<TreeRow> {
        build_flat_list(&self.tree, &self.ui_state, &self.cache)
    }

    pub fn selected_row(&self) -> Option<TreeRow> {
        self.flat_rows().into_iter().nth(self.selected_index)
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let len = self.flat_rows().len();
        if self.selected_index + 1 < len {
            self.selected_index += 1;
        }
    }

    pub fn set_folded(&mut self, value: bool) {
        if let Some(row) = self.selected_row() {
            if row.is_dir {
                self.ui_state.set_folded(&row.path, value);
            }
        }
    }

    /// Space: toggle staging on selected row (file or whole directory)
    pub fn toggle_stage_selected(&mut self) {
        if let Some(row) = self.selected_row() {
            let new_state = row.staging_state.toggle();
            self.set_state_for_path(&row.path, new_state);
        }
    }

    fn set_state_for_path(&mut self, path: &PathBuf, new_state: StagingState) {
        for node in &mut self.tree.nodes {
            if Self::apply_state_recursive(node, path, new_state) {
                break;
            }
        }
    }

    fn apply_state_recursive(
        node: &mut core_lib::tree::TreeNode,
        target: &PathBuf,
        new_state: StagingState,
    ) -> bool {
        use core_lib::tree::TreeNode;
        match node {
            TreeNode::File(f) => {
                if &f.path == target {
                    f.state = new_state;
                    return true;
                }
            }
            TreeNode::Directory(dir) => {
                if &dir.path == target {
                    // Stage/unstage entire subtree
                    for child in &mut dir.children {
                        child.set_state_recursive(new_state);
                    }
                    return true;
                }
                for child in &mut dir.children {
                    if Self::apply_state_recursive(child, target, new_state) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Handle a completed command string like "add ./src" or "u ./**/*.tsx"
    pub fn execute_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        let (verb, pattern) =
            if let Some(rest) = cmd.strip_prefix("add ").or(cmd.strip_prefix("a ")) {
                (StagingState::Staged, rest)
            } else if let Some(rest) = cmd.strip_prefix("unstage ").or(cmd.strip_prefix("u ")) {
                (StagingState::Unstaged, rest)
            } else {
                return;
            };

        // Collect matching paths first to avoid borrow issues
        let rows = self.flat_rows();
        let matching: Vec<PathBuf> = rows
            .iter()
            .filter(|r| !r.is_dir && glob_match(pattern, &r.path))
            .map(|r| r.path.clone())
            .collect();

        for path in matching {
            self.set_state_for_path(&path, verb);
        }
    }

    pub fn open_file_view(&mut self) {
        if let Some(row) = self.selected_row() {
            if !row.is_dir {
                // Fetch the left and right file targets directly from the generated TreeRow
                if let (Some(left), Some(right)) = (&row.left_path, &row.right_path) {
                    self.view_mode = ViewMode::FileView(row.path.clone());

                    if matches!(
                        self.cache.get_diff(&row.path),
                        core_lib::lazy::Lazy::Unstarted
                    ) {
                        self.cache.mark_started(row.path.clone());
                        let _ = self.worker_tx.send(WorkerRequest::ComputeFullDiff {
                            node_path: row.path.clone(),
                            left_path: left.clone(),
                            right_path: right.clone(),
                        });
                    }
                }
            }
        }
    }
}

/// Minimal glob: supports `*` (any chars in segment) and `**` (any path segment)
fn glob_match(pattern: &str, path: &PathBuf) -> bool {
    let path_str = path.to_string_lossy();
    glob_match_str(pattern, &path_str)
}

fn glob_match_str(pattern: &str, s: &str) -> bool {
    // Strip leading "./"
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let s = s.strip_prefix("./").unwrap_or(s);

    let pat_parts: Vec<&str> = pattern.split('/').collect();
    let str_parts: Vec<&str> = s.split('/').collect();
    glob_parts(&pat_parts, &str_parts)
}

fn glob_parts(pat: &[&str], s: &[&str]) -> bool {
    match (pat.first(), s.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            // ** matches zero or more segments
            for i in 0..=s.len() {
                if glob_parts(&pat[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(p), Some(seg)) => {
            if segment_match(p, seg) {
                glob_parts(&pat[1..], &s[1..])
            } else {
                false
            }
        }
        _ => false,
    }
}

fn segment_match(pattern: &str, s: &str) -> bool {
    // Simple * wildcard within a single path segment
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == s;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = s;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            return remaining.ends_with(part);
        } else {
            if let Some(pos) = remaining.find(part) {
                remaining = &remaining[pos + part.len()..];
            } else {
                return false;
            }
        }
    }
    true
}
