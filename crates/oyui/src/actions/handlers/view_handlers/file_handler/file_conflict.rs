//! Inline conflict resolution for merge sessions.
//!
//! Conflicts are rendered as ordinary file lines (with visible markers), so
//! resolving one splices the chosen side into the working file, rewrites it,
//! and recomputes the diff in place.

use std::path::PathBuf;
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
    /// Resolves the conflict under the cursor with `side`, then refreshes.
    fn resolve_conflict(&self, side: Side) {
        if self.reject_read_only() {
            return;
        }

        let Some(line) = self.resolve_cursor().and_then(|c| c.new_line) else {
            self.set_message(MessageLevel::Info, "no conflict under cursor".into());
            return;
        };

        let (has_state, index) = {
            let ui = self.ui.lock();
            match ui.resolve.as_ref() {
                Some(state) => (true, state.conflict_at_line(line)),
                None => (false, None),
            }
        };

        if !has_state {
            self.set_message(MessageLevel::Info, "no conflicts to resolve".into());
            return;
        }
        let Some(index) = index else {
            self.set_message(MessageLevel::Info, "no conflict under cursor".into());
            return;
        };

        let text = {
            let mut ui = self.ui.lock();
            let state = ui.resolve.as_mut().expect("checked above");
            state.set_choice(index, side);
            state.resolved_text()
        };

        let Some(target) = self.write_target.clone() else {
            return;
        };
        if let Err(e) = std::fs::write(&target, &text) {
            self.set_message(MessageLevel::Error, format!("write failed: {e}"));
            return;
        }

        self.refresh_open_diff();

        let label = match side {
            Side::Ours => "ours",
            Side::Theirs => "theirs",
            Side::Both => "both",
        };
        self.set_message(
            MessageLevel::Info,
            format!("conflict {} -> {label}", index + 1),
        );
    }

    /// Recomputes the open file's diff from disk after its content changed.
    fn refresh_open_diff(&self) {
        let Some(path) = self.ui.lock().file_view.current_path.clone() else {
            return;
        };
        let Some((left, right)) = self.tree.read().find_paths(&path) else {
            return;
        };

        let read = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| std::fs::read_to_string(p).unwrap_or_default())
                .unwrap_or_default()
        };
        let old = read(&left);
        let new = read(&right);

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
