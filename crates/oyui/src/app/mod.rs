//! Application state and lifecycle.
pub mod commands;
pub mod draw;
pub mod events;
pub mod input;
pub mod merge;
pub mod merge_stats;
pub mod run;
pub mod ui_state;
pub mod undo;

pub use events::{CommandMode, ExitAction};
use typed_builder::TypedBuilder;
pub use ui_state::UiState;
use ui_state::{Message, MessageLevel};

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
        match event {
            crate::worker::Event::WatchConfigRes(res) => {
                self.config.handle_reload_event(&res.path);
            }
            crate::worker::Event::FileOpened(_) => {
                if let Err(e) = self.config.call_event("file_opened") {
                    self.set_message(MessageLevel::Error, e.to_string());
                }
            }
            crate::worker::Event::AnalysisRes(res) => {
                if let Some(err) = &res.error {
                    self.set_message(MessageLevel::Error, err.clone());
                }
                // A result for a task dropped by a config reload is stale, not
                // an error worth pinning on screen.
                if let Err(e) = self.config.call_task(res.task_id, res.matches.join("\n")) {
                    tracing::warn!("stale analysis result: {e}");
                }
            }
            _ => {}
        }
    }

    /// Shows a transient bottom-bar notification.
    pub(crate) fn set_message(&self, level: MessageLevel, text: String) {
        self.ui.lock().message = Some(Message::new(level, text, Message::DEFAULT_TTL));
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
        // `:help [keybinds]` opens the keybinding overlay.
        let trimmed = cmd.trim();
        if matches!(trimmed, "help" | "help keybinds" | "keybinds") {
            self.ui.lock().help = Some(ui_state::HelpState::default());
            return;
        }

        // Snapshot before a staging command so palette edits are undoable.
        let is_staging = matches!(
            cmd.trim().split_whitespace().next(),
            Some("add" | "a" | "unstage" | "u" | "invert" | "i")
        );
        if is_staging {
            let tree = self.tree.read();
            let snap = crate::app::undo::capture(&tree, &self.cache);
            drop(tree);
            self.ui.lock().undo.new_action(snap);
        }

        let handled = {
            let mut tree = self.tree.write();
            let mut ui = self.ui.lock();
            commands::execute(cmd, &mut tree, &mut ui.tree_view, &self.cache)
        };

        if handled {
            return;
        }

        // Fall back to script-defined commands (`command::register`).
        let mut parts = cmd.trim().splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default();
        if name.is_empty() {
            return;
        }
        let args = parts.next().unwrap_or_default();
        if let Err(e) = self.config.call_command(name, args) {
            self.set_message(MessageLevel::Error, e.to_string());
        }
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
