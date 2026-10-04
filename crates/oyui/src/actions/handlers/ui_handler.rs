//! Script-controlled view options: hint bar and tree row templates.

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

impl UiHintActionsHandler for AppActionsHandler {
    fn set(&self, view: String, format: String) {
        let view = view.to_lowercase();
        if view != "file" && view != "tree" {
            self.set_message(
                crate::app::ui_state::MessageLevel::Error,
                format!("ui::hint: unknown view '{view}'"),
            );
            return;
        }
        self.ui.lock().hint_formats.insert(view, format);
    }

    fn clear(&self, view: String) {
        self.ui.lock().hint_formats.remove(&view.to_lowercase());
    }
}

impl UiStatusActionsHandler for AppActionsHandler {
    fn get(&self) -> String {
        self.ui.lock().status.clone()
    }

    fn set(&self, val: String) {
        self.ui.lock().status = crate::app::ui_state::sanitize_panel_text(&val);
    }
}

impl UiTreeRowFormatActionsHandler for AppActionsHandler {
    fn get(&self) -> String {
        self.ui
            .lock()
            .tree_view
            .tree_row_format
            .clone()
            .unwrap_or_default()
    }

    fn set(&self, val: String) {
        let mut ui = self.ui.lock();
        ui.tree_view.tree_row_format = if val.trim().is_empty() {
            None
        } else {
            Some(val)
        };
    }
}
