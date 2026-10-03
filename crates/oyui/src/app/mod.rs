//! Application state and lifecycle.
pub mod commands;
pub mod draw;
pub mod events;
pub mod input;
pub mod merge;
pub mod merge_stats;
pub mod run;
pub mod ui_state;

pub use events::{CommandMode, ExitAction};
use typed_builder::TypedBuilder;
pub use ui_state::UiState;

use crate::actions::BoxedHandler;
use crate::commands::CommandError;
use crate::config::Config;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::{tasks, EventRegistry};
use parking_lot::{Mutex, RwLock};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(TypedBuilder)]
pub struct App {
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub worker: Arc<EventRegistry>,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub base_path: Option<PathBuf>,
    pub config: Config,
    pub handler: BoxedHandler,
    pub color_mode: TerminalColorMode,
    /// Shared theme: workers read, theme actions write.
    pub theme: Arc<RwLock<ThemeState>>,
    /// UI-only state: main-task confined behind one lock.
    pub ui: Arc<Mutex<UiState>>,
}

impl App {
    pub async fn start(&mut self) -> Result<(), CommandError> {
        self.start_tree_calculation()?;
        self.config.start_watching(&self.worker)?;
        self.run().await?;
        Ok(())
    }

    /// Applies one worker event to the app state.
    fn handle_worker_event(&mut self, event: crate::worker::Event) {
        if let crate::worker::Event::WatchConfigRes(res) = event {
            self.config.handle_reload_event(&res.path);
        }
    }

    /// Processes pending worker events.
    pub fn tick(&mut self) {
        while let Ok(event) = self.worker.try_recv() {
            self.handle_worker_event(event);
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&self) {
        let _ = self.worker.shutdown().await;
    }

    #[tracing::instrument(skip_all, fields(cmd = cmd))]
    pub fn execute_command(&mut self, cmd: &str) {
        let mut tree = self.tree.write();
        let mut ui = self.ui.lock();
        commands::execute(cmd, &mut tree, &mut ui.tree_view, &self.cache);
    }

    pub fn start_tree_calculation(&self) -> eyre::Result<()> {
        self.worker
            .send(tasks::calculate_file_tree::CalculateFileTreeReq {
                left: self.left_path.clone(),
                right: self.right_path.clone(),
            })?;
        Ok(())
    }
}
