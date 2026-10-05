use crate::actions::handlers::AppActionsHandler;
use crate::actions::{GlobalActionsHandler, GlobalConfirmMergeWindowEnabledActionsHandler};
use crate::app::ui_state::{HelpState, Message, MessageLevel};
use crate::app::CommandMode;

impl GlobalActionsHandler for AppActionsHandler {
    fn quit(&self) {
        self.ui.lock().should_quit = true;
    }

    fn confirm(&self) {
        let enabled = self.ui.lock().confirm_merge_window_enabled;
        if enabled {
            // Refresh staging counts now so rendering stays read-only.
            crate::app::merge_stats::sync_line_selections(&self.tree, &self.cache);
            self.ui.lock().command_mode = CommandMode::ConfirmMerge;
            return;
        }
        self.execute_merge();
    }

    fn execute_merge(&self) {
        let mut tree = self.tree.write();
        let res = crate::app::merge::confirm_and_write(&mut tree, &self.right_path, &self.cache);
        match res {
            Ok(crate::app::events::ExitAction::QuitAndMerge) => {
                self.ui.lock().should_quit = true;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Merge failed: {}", e);
            }
        }
    }

    fn open_command_mode(&self) {
        self.ui.lock().command_mode = CommandMode::Active(String::new());
    }

    fn left_path(&self) -> String {
        self.left_path.display().to_string()
    }

    fn right_path(&self) -> String {
        self.right_path.display().to_string()
    }

    fn base_path(&self) -> String {
        self.base_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    }

    fn view(&self) -> String {
        match self.ui.lock().current {
            crate::view::ViewKind::File => "file".into(),
            crate::view::ViewKind::Tree => "tree".into(),
        }
    }

    fn algorithm(&self) -> String {
        format!("{:?}", self.algorithm).to_lowercase()
    }

    fn switch(&self, view: String) {
        let target = match view.to_lowercase().as_str() {
            "file" => crate::view::ViewKind::File,
            "tree" => crate::view::ViewKind::Tree,
            other => {
                self.set_message(
                    MessageLevel::Error,
                    format!("global::switch: unknown view '{other}' (expected 'file' or 'tree')"),
                );
                return;
            }
        };
        self.ui.lock().current = target;
    }

    fn command(&self, cmd: String) {
        let is_staging = matches!(
            cmd.trim().split_whitespace().next(),
            Some("add" | "a" | "unstage" | "u" | "invert" | "i")
        );
        if is_staging {
            self.push_undo_snapshot();
        }
        let mut tree = self.tree.write();
        let mut ui = self.ui.lock();
        crate::app::commands::execute(&cmd, &mut tree, &mut ui.tree_view, &self.cache);
    }

    fn clear_error(&self) {
        *self.error.write() = None;
    }

    fn notify(&self, msg: String) {
        self.set_message(MessageLevel::Info, msg);
    }

    fn warn(&self, msg: String) {
        self.set_message(MessageLevel::Warn, msg);
    }

    fn error(&self, msg: String) {
        self.set_message(MessageLevel::Error, msg);
    }

    fn clear_message(&self) {
        self.ui.lock().message = None;
    }

    fn undo(&self) {
        let Some(snap) = self.ui.lock().undo.take_undo() else {
            self.set_message(MessageLevel::Info, "nothing to undo".into());
            return;
        };
        let current = self.capture_snapshot();
        self.ui.lock().undo.put_redo(current);
        self.restore_snapshot(snap);
        self.set_message(MessageLevel::Info, "undo".into());
    }

    fn redo(&self) {
        let Some(snap) = self.ui.lock().undo.take_redo() else {
            self.set_message(MessageLevel::Info, "nothing to redo".into());
            return;
        };
        let current = self.capture_snapshot();
        self.ui.lock().undo.record(current);
        self.restore_snapshot(snap);
        self.set_message(MessageLevel::Info, "redo".into());
    }

    fn copy(&self, text: String) {
        match super::clipboard::osc52_copy(&text) {
            Ok(()) => self.set_message(
                MessageLevel::Info,
                format!("copied {} bytes to clipboard", text.len()),
            ),
            Err(e) => self.set_message(MessageLevel::Error, format!("clipboard failed: {e}")),
        }
    }

    fn help(&self) {
        let mut ui = self.ui.lock();
        ui.help = match ui.help {
            Some(_) => None,
            None => Some(HelpState::default()),
        };
    }
}

impl AppActionsHandler {
    /// Sets a transient bottom-bar message with a default lifetime.
    pub(crate) fn set_message(&self, level: MessageLevel, text: String) {
        self.ui.lock().message = Some(Message::new(level, text, Message::DEFAULT_TTL));
    }
}

impl GlobalConfirmMergeWindowEnabledActionsHandler for AppActionsHandler {
    fn get(&self) -> bool {
        self.ui.lock().confirm_merge_window_enabled
    }

    fn set(&self, val: bool) {
        self.ui.lock().confirm_merge_window_enabled = val;
    }
}
