//! Inline conflict folding for merge sessions.
//!
//! Selecting a side (`o`/`T`/`B`) folds the conflict to a summary marker
//! instead of rewriting the file: the markers stay on disk, so a choice can be
//! revisited and (for jj) the conflict can be confirmed as-is. Pressing `o`
//! again expands the conflict.
//!
//! Folding is a rendering concern of the file view model (`ConflictRegions`),
//! so the underlying diff and its staging selections are never touched.

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::app::ui_state::MessageLevel;
use crate::diff::Side;

impl ViewFileConflictActionsHandler for AppActionsHandler {
    fn ours(&self) {
        self.resolve_conflict(Side::Ours);
    }

    fn theirs(&self) {
        self.resolve_conflict(Side::Theirs);
    }

    fn both(&self) {
        self.resolve_conflict(Side::Both);
    }
}

impl AppActionsHandler {
    /// Folds (or, for `ours`, re-expands) the conflict under the cursor.
    fn resolve_conflict(&self, side: Side) {
        if self.reject_read_only() {
            return;
        }

        let Some(line) = self.resolve_cursor().and_then(|c| c.new_line) else {
            self.set_message(MessageLevel::Info, "no conflict under cursor".into());
            return;
        };

        let outcome = {
            let mut ui = self.ui.lock();
            let located = ui
                .resolve
                .as_ref()
                .and_then(|state| state.conflict_at_line(line));
            let Some(index) = located else {
                drop(ui);
                self.set_message(MessageLevel::Info, "no conflict under cursor".into());
                return;
            };

            let state = ui.resolve.as_mut().expect("located implies state");
            if side == Side::Ours && state.is_folded(index) {
                state.set_folded(index, false);
                (index, false)
            } else {
                state.set_choice(index, side);
                state.set_folded(index, true);
                (index, true)
            }
        };

        // The diff content is unchanged; only the row layout needs refreshing.
        self.ui.lock().file_view.mark_dirty();

        let (index, folded) = outcome;
        self.set_message(
            MessageLevel::Info,
            format!(
                "conflict {} {}",
                index + 1,
                if folded { "folded" } else { "expanded" }
            ),
        );
    }
}
