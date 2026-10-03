use crate::actions::handlers::AppActionsHandler;
use crate::actions::{GlobalActionsHandler, GlobalConfirmMergeWindowEnabledActionsHandler};
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
}

impl GlobalConfirmMergeWindowEnabledActionsHandler for AppActionsHandler {
    fn get(&self) -> bool {
        self.ui.lock().confirm_merge_window_enabled
    }

    fn set(&self, val: bool) {
        self.ui.lock().confirm_merge_window_enabled = val;
    }
}
