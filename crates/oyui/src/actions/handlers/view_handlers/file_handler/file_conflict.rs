//! Inline conflict folding for merge sessions.
//!
//! Selecting a side (`o`/`T`/`B`) folds the conflict to a summary marker
//! instead of rewriting the file: the markers stay on disk, so a choice can be
//! revisited and (for jj) the conflict can be confirmed as-is. Pressing `o`
//! again expands the conflict.
//!
//! Folding only changes the *displayed* content inside a conflict block, so the
//! hunk-staging selections are carried across the recomputation (matched by
//! line kind and text), keeping the two mechanisms coherent.

use std::collections::HashMap;
use std::sync::Arc;

use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;
use crate::app::ui_state::MessageLevel;
use crate::diff::staging::is_file_staged_default;
use crate::diff::{DiffLine, DiffResult, FileDiff, LineSelections, Side};

/// Staged state of a modifiable line, keyed by kind and text.
type Captured = HashMap<(char, String), bool>;

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

    /// Recomputes the open file's diff from `new` (display) content, carrying
    /// the hunk-staging selections across the recomputation.
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
        let default = is_file_staged_default(&self.tree, &path);

        let captured = match self.cache.diffs.get(&path).as_deref() {
            Some(DiffResult::Text(diff)) => capture(diff, default),
            _ => Captured::new(),
        };

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
                apply_selections(diff, &captured, default);
            }
        });

        let mut ui = self.ui.lock();
        ui.file_view.mark_dirty();
        ui.tree_view.mark_dirty();
    }
}

/// Records the staged state of every modifiable line, keyed by kind and text.
fn capture(diff: &FileDiff, default: bool) -> Captured {
    let new_lines: Vec<&str> = diff.new_file_content.lines().collect();
    let old_lines: Vec<&str> = diff.old_file_content.lines().collect();
    let mut map = Captured::new();
    let mut idx = 0;

    for hunk in &diff.hunks {
        for line in &hunk.lines {
            if let Some(key) = selection_key(line, &new_lines, &old_lines) {
                map.insert(key, diff.line_selections.get(idx, default));
            }
            idx += 1;
        }
    }
    map
}

/// Re-applies captured selections to a freshly computed diff.
fn apply_selections(diff: &mut FileDiff, captured: &Captured, default: bool) {
    let total: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
    diff.line_selections.ensure_size(total, default);

    let mut updates = Vec::new();
    {
        let new_lines: Vec<&str> = diff.new_file_content.lines().collect();
        let old_lines: Vec<&str> = diff.old_file_content.lines().collect();
        let mut idx = 0;
        for hunk in &diff.hunks {
            for line in &hunk.lines {
                if let Some(key) = selection_key(line, &new_lines, &old_lines) {
                    if let Some(&staged) = captured.get(&key) {
                        updates.push((idx, staged));
                    }
                }
                idx += 1;
            }
        }
    }

    for (idx, staged) in updates {
        diff.line_selections.set(idx, staged);
    }
}

/// Key identifying a modifiable line across diff recomputations.
fn selection_key(
    line: &DiffLine,
    new_lines: &[&str],
    old_lines: &[&str],
) -> Option<(char, String)> {
    match line {
        DiffLine::Addition { new_line_idx, .. } => Some((
            '+',
            new_lines
                .get(*new_line_idx)
                .copied()
                .unwrap_or("")
                .to_string(),
        )),
        DiffLine::Deletion { old_line_idx, .. } => Some((
            '-',
            old_lines
                .get(*old_line_idx)
                .copied()
                .unwrap_or("")
                .to_string(),
        )),
        DiffLine::Context { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{Hunk, HunkMarker};

    fn diff() -> FileDiff {
        FileDiff {
            old_file_content: Arc::from("keep\nold"),
            new_file_content: Arc::from("keep\nnew"),
            hunks: vec![Hunk {
                before_lines: 1..2,
                after_lines: 1..2,
                lines: vec![DiffLine::Deletion {
                    old_line_idx: 1,
                    inline_highlights: Vec::new(),
                }],
                marker: HunkMarker::None,
            }],
            line_selections: LineSelections::default(),
        }
    }

    /// A selection survives a recomputation that leaves the line unchanged.
    #[test]
    fn staging_survives_recompute() {
        let mut before = diff();
        before.line_selections = LineSelections::new(1, true);
        let captured = capture(&before, false);

        let mut after = diff();
        apply_selections(&mut after, &captured, false);
        assert_eq!(after.line_selections.get(0, false), true);
    }
}
