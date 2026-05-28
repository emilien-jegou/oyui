use crate::actions::handlers::AppActionsHandler;
use crate::actions::GlobalActionsHandler;
use crate::app::CommandMode;

impl GlobalActionsHandler for AppActionsHandler {
    fn quit(&self) {
        let mut state = self.state.write();
        state.should_quit = true;
    }

    fn confirm(&self) {
        let mut state = self.state.write();
        state.confirm_merge_triggered = true;
    }

    fn open_command_mode(&self) {
        let mut state = self.state.write();
        state.command_mode = CommandMode::Active(String::new());
    }
}
