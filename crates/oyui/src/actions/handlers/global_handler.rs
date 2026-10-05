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
        // Read-only sessions (e.g. `oyui diff --no-write`) simply exit.
        let Some(target) = self.write_target.clone() else {
            self.ui.lock().should_quit = true;
            return;
        };

        // Merge sessions resolve through the conflict overlay (or the
        // synthesized result); the two-way staging write would clobber the
        // target, so it must not run here.
        if self.operation == crate::app::Operation::Merge {
            let snapshot = {
                let ui = self.ui.lock();
                ui.resolve.as_ref().map(|s| (s.count(), s.resolved_text()))
            };
            match snapshot {
                Some((count, _)) if count > 0 => {
                    self.ui.lock().resolve_open = true;
                    self.set_message(
                        MessageLevel::Info,
                        "resolve conflicts, then press enter".into(),
                    );
                }
                Some((_, text)) => self.write_result(&target, text),
                None => {
                    self.ui.lock().should_quit = true;
                }
            }
            return;
        }

        let mut tree = self.tree.write();
        let res = crate::app::merge::confirm_and_write(&mut tree, &target, &self.cache);
        match res {
            Ok(crate::app::events::ExitAction::QuitAndMerge) => {
                self.ui.lock().should_quit = true;
            }
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Merge failed: {}", e);
                self.set_message(MessageLevel::Error, format!("write failed: {e}"));
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

    fn operation(&self) -> String {
        self.operation.as_str().to_string()
    }

    fn writable(&self) -> bool {
        self.write_target.is_some()
    }

    fn conflict_count(&self) -> u32 {
        self.ui
            .lock()
            .resolve
            .as_ref()
            .map_or(0, |r| r.count() as u32)
    }

    fn resolve(&self) {
        let mut ui = self.ui.lock();
        if ui.resolve.is_some() {
            ui.resolve_open = !ui.resolve_open;
        } else {
            ui.message = Some(Message::new(
                MessageLevel::Info,
                "no conflicts to resolve".into(),
                Message::DEFAULT_TTL,
            ));
        }
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
            if self.reject_read_only() {
                return;
            }
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
        if self.reject_read_only() {
            return;
        }
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
        if self.reject_read_only() {
            return;
        }
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

    /// True when the session must not mutate staging (read-only diff).
    pub(crate) fn is_read_only(&self) -> bool {
        self.write_target.is_none()
    }

    /// Rejects a staging action in a read-only session, warning the user.
    pub(crate) fn reject_read_only(&self) -> bool {
        if self.is_read_only() {
            self.set_message(MessageLevel::Info, "read-only session".into());
            true
        } else {
            false
        }
    }

    /// Writes `text` to `target`, quitting the session on success.
    pub(crate) fn write_result(&self, target: &std::path::Path, text: String) {
        match std::fs::write(target, text) {
            Ok(()) => {
                self.set_message(MessageLevel::Info, format!("wrote {}", target.display()));
                self.ui.lock().should_quit = true;
            }
            Err(e) => self.set_message(MessageLevel::Error, format!("write failed: {e}")),
        }
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
