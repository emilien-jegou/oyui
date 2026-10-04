//! Runtime settings shared by both panes.

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

impl SettingsScrolloffActionsHandler for AppActionsHandler {
    fn get(&self) -> u32 {
        self.ui.lock().file_view.scrolloff as u32
    }

    fn set(&self, val: u32) {
        let mut ui = self.ui.lock();
        ui.file_view.scrolloff = val as usize;
        ui.tree_view.scrolloff = val as usize;
    }
}

impl SettingsContextLinesActionsHandler for AppActionsHandler {
    fn get(&self) -> u32 {
        self.ui.lock().file_view.context_lines as u32
    }

    fn set(&self, val: u32) {
        let mut ui = self.ui.lock();
        ui.file_view.context_lines = val as usize;
        ui.file_view.mark_dirty();
    }
}
