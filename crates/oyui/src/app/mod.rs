pub mod commands;
pub mod events;
pub mod merge;
pub mod worker;

pub use events::{CommandMode, ExitAction};

use crate::view::View;
use core_lib::diff_cache::DiffCache;
use core_lib::tree::{FileTree, TreeNode};
use core_lib::worker::{AsyncWorkerEvent, WorkerRequest};
use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;

pub struct App {
    pub tree: FileTree,
    pub cache: DiffCache,
    pub view: View,

    pub command_mode: CommandMode,
    pub should_quit: bool,

    pub worker_tx: Sender<WorkerRequest>,
    pub worker_rx: Receiver<AsyncWorkerEvent>,

    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub base_path: Option<PathBuf>,
}

impl App {
    pub fn new(worker_tx: Sender<WorkerRequest>, worker_rx: Receiver<AsyncWorkerEvent>) -> Self {
        Self {
            tree: FileTree::new(),
            cache: DiffCache::default(),
            view: View::new(),
            command_mode: CommandMode::Normal,
            should_quit: false,
            worker_tx,
            worker_rx,

            left_path: None,
            right_path: None,
            base_path: None,
        }
    }

    pub fn tick(&mut self) {
        worker::process_events(&self.worker_rx, &self.worker_tx, &mut self.cache);
    }

    pub fn execute_command(&mut self, cmd: &str) {
        commands::execute(cmd, &mut self.tree, &self.view.tree_view, &self.cache);
    }

    pub fn confirm_merge(&mut self) -> Result<ExitAction, Box<dyn std::error::Error>> {
        merge::confirm_and_write(&mut self.tree, &mut self.should_quit)
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
