use crate::actions::handlers::AppActionsHandler;
use crate::actions::{GlobalActionsHandler, GlobalConfirmMergeWindowEnabledActionsHandler};
use crate::app::CommandMode;

impl GlobalActionsHandler for AppActionsHandler {
    fn quit(&self) {
        let mut state = self.state.write();
        state.should_quit = true;
    }

    fn confirm(&self) {
        let enabled = self.state.read().confirm_merge_window_enabled;
        if enabled {
            self.state.write().command_mode = CommandMode::ConfirmMerge;
        } else {
            self.execute_merge();
        }
    }

    fn execute_merge(&self) {
        let mut tree = self.tree.write();
        let cache = self.cache.read();
        let mut should_quit = false;
        let res = crate::app::merge::confirm_and_write(
            &mut tree,
            &mut should_quit,
            &self.right_path,
            &cache,
        );
        if should_quit {
            self.state.write().should_quit = true;
        }
        if let Err(e) = res {
            tracing::error!("Merge failed: {}", e);
        }
    }

    fn open_command_mode(&self) {
        let mut state = self.state.write();
        state.command_mode = CommandMode::Active(String::new());
    }
}

impl GlobalConfirmMergeWindowEnabledActionsHandler for AppActionsHandler {
    fn get(&self) -> bool {
        self.state.read().confirm_merge_window_enabled
    }

    fn set(&self, val: bool) {
        self.state.write().confirm_merge_window_enabled = val;
    }
}
