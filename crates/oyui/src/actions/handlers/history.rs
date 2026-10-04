//! Staging snapshot capture/restore backing undo/redo.

use crate::actions::handlers::AppActionsHandler;
use crate::app::undo::StagingSnapshot;

impl AppActionsHandler {
    /// Captures the current staging state (tree + every cached text diff).
    pub(crate) fn capture_snapshot(&self) -> StagingSnapshot {
        crate::app::undo::capture(&self.tree.read(), &self.cache)
    }

    /// Records an undo point for a fresh staging mutation.
    pub(crate) fn push_undo_snapshot(&self) {
        let snap = self.capture_snapshot();
        self.ui.lock().undo.new_action(snap);
    }

    /// Restores a snapshot's tree and cached diffs.
    pub(crate) fn restore_snapshot(&self, snap: StagingSnapshot) {
        crate::app::undo::restore(&mut self.tree.write(), &self.cache, snap);
        let mut ui = self.ui.lock();
        ui.tree_view.mark_dirty();
        ui.file_view.mark_dirty();
    }
}
