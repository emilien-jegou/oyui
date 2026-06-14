use crate::actions::handlers::AppActionsHandler;
use crate::actions::{GlobalActionsHandler, GlobalConfirmMergeWindowEnabledActionsHandler};
use crate::app::CommandMode;
use std::sync::atomic::Ordering;

impl GlobalActionsHandler for AppActionsHandler {
    fn quit(&self) {
        self.state.should_quit.store(true, Ordering::Relaxed);
    }

    fn confirm(&self) {
        let enabled = self
            .state
            .confirm_merge_window_enabled
            .load(Ordering::Relaxed);
        if enabled {
            *self.state.command_mode.write() = CommandMode::ConfirmMerge;
        } else {
            self.execute_merge();
        }
    }

    fn execute_merge(&self) {
        let mut tree = self.tree.write();
        let mut should_quit = false;
        let res = crate::app::merge::confirm_and_write(
            &mut tree,
            &mut should_quit,
            &self.right_path,
            &self.cache,
        );
        if should_quit {
            self.state.should_quit.store(true, Ordering::Relaxed);
        }
        if let Err(e) = res {
            tracing::error!("Merge failed: {}", e);
        }
    }

    fn open_command_mode(&self) {
        *self.state.command_mode.write() = CommandMode::Active(String::new());
    }
}

impl GlobalConfirmMergeWindowEnabledActionsHandler for AppActionsHandler {
    fn get(&self) -> bool {
        self.state
            .confirm_merge_window_enabled
            .load(Ordering::Relaxed)
    }

    fn set(&self, val: bool) {
        self.state
            .confirm_merge_window_enabled
            .store(val, Ordering::Relaxed);
    }
}
