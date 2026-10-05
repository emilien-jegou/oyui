//! Inline conflict folding for merge sessions.
//!
//! Selecting a side (`o`/`T`/`B`) folds the conflict to a summary marker
//! instead of rewriting the file: the markers stay on disk, so a choice can be
//! revisited and (for jj) the conflict can be confirmed as-is. Pressing `o`
//! again expands the conflict.

use std::sync::Arc;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::app::ui_state::MessageLevel;
use crate::diff::{DiffResult, LineSelections, Side};

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

        let index = {
            let ui = self.ui.lock();
            ui.resolve
                .as_ref()
                .and_then(|state| state.conflict_at_line(line))
        };
        let Some(index) = index else {
            self.set_message(MessageLevel::Info, "no conflict under cursor".into());
            return;
        };

        let folded = {
            let mut ui = self.ui.lock();
            let state = ui.resolve.as_mut().expect("conflict index implies state");
            if side == Side::Ours && state.is_folded(index) {
                // `o` on a folded conflict expands it again.
                state.set_folded(index, false);
                false
            } else {
                state.set_choice(index, side);
                state.set_folded(index, true);
                true
            }
        };

        let text = self
            .ui
            .lock()
            .resolve
            .as_ref()
            .map(|state| state.display_text())
            .unwrap_or_default();
        self.refresh_open_with(text);

        self.set_message(
            MessageLevel::Info,
            format!(
                "conflict {} {}",
                index + 1,
                if folded { "folded" } else { "expanded" }
            ),
        );
    }

    /// Recomputes the open file's diff from `new` (display) content.
    fn refresh_open_with(&self, new: String) {
        let Some(path) = self.ui.lock().file_view.current_path.clone() else {
            return;
        };
        let Some((left, _right)) = self.tree.read().find_paths(&path) else {
            return;
        };
        let old = left
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();

        let Ok(hunks) =
            crate::worker::tasks::full_diff::compute(&self.algorithm, &old, &new, &path)
        else {
            return;
        };

        self.cache.diffs.update(&path, |result| {
            if let DiffResult::Text(diff) = result {
                diff.old_file_content = Arc::from(old.as_str());
                diff.new_file_content = Arc::from(new.as_str());
                diff.hunks = hunks;
                diff.line_selections = LineSelections::default();
            }
        });

        let mut ui = self.ui.lock();
        ui.file_view.mark_dirty();
        ui.tree_view.mark_dirty();
    }
}
