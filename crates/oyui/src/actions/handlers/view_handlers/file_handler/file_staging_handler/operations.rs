//! Staging action handlers that operate on the current file's diff.

use super::staging_session::StagingSession;
use super::staging_sync::{invert_text_diff_staging, toggle_stage_hunk_in_diff};
use crate::diff::staging::is_file_staged_default;
use crate::diff::{FileDiff, HunkMarker};
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::path::PathBuf;

pub fn toggle_hunk(session: &StagingSession, hunk_idx: usize) {
    session.mutate_diff(|diff, tree| {
        toggle_stage_hunk_in_diff(tree, &session.path, diff, hunk_idx, true);
    });
}

pub fn toggle_conflict_at_cursor(session: &StagingSession) -> bool {
    use crate::diff::Side;

    // Capture the anchor first: the cursor acts as the anchor point, so the
    // view shifts to keep it at the same screen position across the fold.
    let anchor = {
        let ui = session.ui.lock();
        let line = ui
            .file_view
            .line_mapping(&session.path)
            .and_then(|m| m.get(session.current_row_idx).copied());
        let (row, offset) = ui
            .file_view
            .scroll_states
            .get(&session.path)
            .map(|st| (st.selected().unwrap_or(0), st.offset()))
            .unwrap_or((0, 0));
        line.map(|l| (l, row.saturating_sub(offset)))
    };
    let Some((line, screen_offset)) = anchor else {
        return false;
    };

    let (index, folded) = {
        let ui = session.ui.lock();
        match ui.resolve.as_ref().and_then(|s| {
            s.conflict_at_line(line).map(|i| (i, s.is_folded(i)))
        }) {
            Some(v) => v,
            None => return false,
        }
    };

    // Logical line to keep under the cursor (resolved after the mutation,
    // since kept lines depend on the new choice): a kept line survives the
    // fold, anything hidden anchors to the range start.
    if folded {
        // Anything folded unfolds back to the markers: the frame rows and
        // the kept center lines alike. Conflict hunks are not stageable, so
        // space here never toggles staging.
        let mut ui = session.ui.lock();
        if let Some(state) = ui.resolve.as_mut() {
            state.reset_choice(index);
            state.set_folded(index, false);
        }
        ui.file_view.mark_dirty();
        drop(ui);
        clear_conflict_hunk_markers(session, index);
        reanchor_cursor(session, anchor_target(session, index, line), screen_offset);
        return true;
    }

    // Unfolded: pick the side under the cursor (ours vs theirs), default ours.
    let side = {
        let ui = session.ui.lock();
        let state = ui.resolve.as_ref().expect("checked above");
        conflict_side_at_line(state, index, line).unwrap_or(Side::Ours)
    };
    {
        let mut ui = session.ui.lock();
        if let Some(state) = ui.resolve.as_mut() {
            state.set_choice(index, side);
            state.set_folded(index, true);
        }
        ui.file_view.mark_dirty();
    }
    clear_conflict_hunk_markers(session, index);
    reanchor_cursor(session, anchor_target(session, index, line), screen_offset);
    true
}

/// Resets split markers on hunks overlapping conflict `index`.
///
/// `t`/`s` can no longer create them inside conflicts, but a stale
/// `LineToggle`/`HunkSplit` marker (an earlier split, an undo restore) would
/// linger as a `▶`/`◣` glyph on kept lines. Hunk markers are meaningless
/// inside conflicts, so folding normalizes them away.
fn clear_conflict_hunk_markers(session: &StagingSession, index: usize) {
    use crate::diff::{DiffLine, HunkMarker};

    let range = {
        let ui = session.ui.lock();
        ui.resolve
            .as_ref()
            .and_then(|s| s.conflicts.marker_ranges().get(index).cloned())
    };
    let Some(range) = range else { return };
    session.mutate_diff(|diff, _tree| {
        for hunk in &mut diff.hunks {
            let overlaps = hunk.lines.iter().any(|line| match line {
                DiffLine::Context { new_line_idx, .. }
                | DiffLine::Addition { new_line_idx, .. } => range.contains(new_line_idx),
                DiffLine::Deletion { .. } => false,
            });
            if overlaps {
                hunk.marker = HunkMarker::None;
            }
        }
    });
}

/// Logical line to keep under the cursor after a (un)fold, given the new
/// choice: a kept line survives, anything hidden anchors to the range start
/// (which both frame rows map to).
fn anchor_target(session: &StagingSession, index: usize, line: usize) -> usize {
    let ui = session.ui.lock();
    match ui.resolve.as_ref() {
        Some(state) if state.keeps_line(index, line) => line,
        Some(state) => state
            .conflicts
            .marker_ranges()
            .get(index)
            .map(|r| r.start)
            .unwrap_or(line),
        None => line,
    }
}

/// Recomputes the row layout and moves the cursor to `target`, shifting the
/// view so the cursor keeps its screen position (same anchoring as fold
/// toggles: `row - screen_offset`, clamped into range).
fn reanchor_cursor(session: &StagingSession, target: usize, screen_offset: usize) {
    let Some(crate::diff::DiffResult::Text(diff)) =
        session.cache.diffs.get(&session.path).as_deref().cloned()
    else {
        return;
    };
    let mut ui = session.ui.lock();
    ui.file_view.conflict_regions = ui.resolve.as_ref().map(|r| r.regions()).unwrap_or_default();
    ui.file_view.recompute_view_model(&diff);
    let row = ui
        .file_view
        .line_mapping(&session.path)
        .and_then(|m| m.iter().position(|&l| l == target))
        .unwrap_or(0);
    let max_row = ui.file_view.row_count(&session.path).saturating_sub(1);
    let row = row.min(max_row);
    let state = ui
        .file_view
        .scroll_states
        .entry(session.path.clone())
        .or_default();
    state.select(Some(row));
    *state.offset_mut() = row.saturating_sub(screen_offset);
}

fn conflict_side_at_line(
    state: &crate::app::ui_state::ResolveState,
    index: usize,
    line: usize,
) -> Option<crate::diff::Side> {
    use crate::diff::{conflict::Segment, Side};
    let range_start = state.conflicts.marker_ranges().get(index)?.start;
    // Walk conflicts in order to reach `index`.
    let mut idx = 0;
    for seg in &state.conflicts.segments {
        if let Segment::Conflict(c) = seg {
            if idx == index {
                // Layout of marker block: `<<<<<<<`(1) + ours + [base] + `=======`(1) + theirs + `>>>>>>>`(1).
                // The `=======` separator belongs to the side below it.
                let ours_end = range_start + 1 + c.ours.len();
                let base_len = c.base.as_ref().map(|b| b.len() + 1).unwrap_or(0);
                let sep = ours_end + base_len;
                if line < ours_end {
                    return Some(Side::Ours);
                } else if line >= sep {
                    return Some(Side::Theirs);
                } else {
                    return Some(Side::Ours);
                }
            }
            idx += 1;
        }
    }
    None
}

pub fn toggle_stage_at_cursor(session: &StagingSession) {
    if toggle_conflict_at_cursor(session) {
        return;
    }

    // Merge sessions resolve through conflict choices; the staging fallback
    // would silently do nothing on confirm.
    if session.is_merge() {
        let mut ui = session.ui.lock();
        ui.message = Some(crate::app::ui_state::Message::new(
            crate::app::ui_state::MessageLevel::Info,
            "merge mode: staging has no effect".to_string(),
            crate::app::ui_state::Message::DEFAULT_TTL,
        ));
        return;
    }

    let ui = session.ui.lock();
    let mappings = ui.file_view.row_to_hunk(&session.path);
    let mut hidx =
        mappings.and_then(|mapping| mapping.get(session.current_row_idx).copied().flatten());

    if hidx.is_none() {
        if let Some(mapping) = mappings {
            let r = session.current_row_idx;
            for d in 1..=4 {
                if let Some(row) = r.checked_add(d) {
                    if let Some(&Some(hunk_idx)) = mapping.get(row) {
                        hidx = Some(hunk_idx);
                        break;
                    }
                }
                if let Some(row) = r.checked_sub(d) {
                    if let Some(&Some(hunk_idx)) = mapping.get(row) {
                        hidx = Some(hunk_idx);
                        break;
                    }
                }
            }
        }
    }
    drop(ui);

    if let Some(hunk_idx) = hidx {
        let is_line_toggle = {
            if let Some(crate::diff::DiffResult::Text(diff)) =
                session.cache.diffs.get(&session.path).as_deref()
            {
                diff.hunks
                    .get(hunk_idx)
                    .map(|h| h.marker == HunkMarker::LineToggle)
                    .unwrap_or(false)
            } else {
                false
            }
        };
        session.mutate_diff(|diff, tree| {
            toggle_stage_hunk_in_diff(tree, &session.path, diff, hunk_idx, !is_line_toggle);
        });
    }
}

/// True when the cursor sits on any conflict line (markers, ours, base or
/// theirs content, folded or unfolded).
///
/// Conflict hunks are not stageable — space there only folds/unfolds, and
/// per-line ops must ignore them — so the colors of a conflict never reflect
/// staging and identical conflict states always look identical.
fn cursor_on_conflict_line(session: &StagingSession) -> bool {
    let line_opt = {
        let ui = session.ui.lock();
        ui.file_view
            .line_mapping(&session.path)
            .and_then(|m| m.get(session.current_row_idx).copied())
    };
    let Some(line) = line_opt else {
        return false;
    };
    session
        .ui
        .lock()
        .resolve
        .as_ref()
        .and_then(|s| s.conflict_at_line(line))
        .is_some()
}

pub fn toggle_single_line_at_cursor(session: &StagingSession) {
    // Conflict hunks are not stageable: space there only folds/unfolds.
    if cursor_on_conflict_line(session) {
        return;
    }
    let (hidx, visual_start) = match (session.hunk_idx, session.hunk_visual_start) {
        (Some(hi), Some(vs)) => (hi, vs),
        _ => return,
    };

    session.mutate_diff(|diff, tree| {
        let (contiguous_prev, contiguous_next) = check_hunk_contiguity(diff, hidx);
        let hunk_len = diff.hunks.get(hidx).map(|h| h.lines.len()).unwrap_or(0);
        let is_line_toggle = diff.hunks[hidx].marker == HunkMarker::LineToggle;

        if hunk_len == 1 && is_line_toggle {
            handle_untoggle_single_line(
                diff,
                tree,
                &session.path,
                hidx,
                contiguous_prev,
                contiguous_next,
            );
            return;
        }

        let line_within_hunk = session.current_row_idx.saturating_sub(visual_start);
        let target_hunk_idx = isolate_line_as_toggle_hunk(diff, hidx, line_within_hunk);
        toggle_stage_hunk_in_diff(tree, &session.path, diff, target_hunk_idx, false);
    });
}

pub fn split_hunk_at_cursor(session: &StagingSession) {
    // Never split inside a conflict: conflict hunks are not stageable.
    if cursor_on_conflict_line(session) {
        return;
    }
    let (hidx, visual_start) = match (session.hunk_idx, session.hunk_visual_start) {
        (Some(hi), Some(vs)) => (hi, vs),
        _ => return,
    };

    let split_idx = session.current_row_idx.saturating_sub(visual_start);

    session.mutate_diff(|diff, tree| {
        if split_idx == 0 && hidx > 0 {
            handle_boundary_split(diff, tree, &session.path, hidx);
            return;
        }
        if split_idx > 0 {
            diff.split_hunk(hidx, split_idx, HunkMarker::HunkSplit);
        }
    });
}

pub fn split_at(session: &StagingSession, hunk_idx: usize, line_idx: usize) {
    session.mutate_diff(|diff, _tree| {
        diff.split_hunk(hunk_idx, line_idx, HunkMarker::HunkSplit);
    });
}

pub fn join_at_cursor(session: &StagingSession) {
    let Some(hidx) = session.hunk_idx else {
        return;
    };
    session.mutate_diff(|diff, tree| {
        let default = is_file_staged_default(tree, &session.path);
        diff.join_hunk(hidx, true, default);
        crate::diff::staging::update_tree_staging_state(tree, &session.path, diff, default);
    });
}

/// Deterministically stages or unstages a hunk by index.
pub fn set_hunk_staged(session: &StagingSession, hunk_idx: usize, staged: bool) {
    session.mutate_diff(|diff, tree| {
        let default = is_file_staged_default(tree, &session.path);
        diff.set_hunk(hunk_idx, true, default, staged);
        crate::diff::staging::update_tree_staging_state(tree, &session.path, diff, default);
    });
}

/// Stages or unstages every modifiable line in the open file.
pub fn set_all(session: &StagingSession, staged: bool) {
    session.mutate_diff(|diff, tree| {
        let default = is_file_staged_default(tree, &session.path);
        diff.set_all_staging(default, staged);
        crate::diff::staging::update_tree_staging_state(tree, &session.path, diff, default);
    });
}

pub fn invert_staging(session: &StagingSession) {
    let has_text_diff = matches!(
        session.cache.diffs.get(&session.path).as_deref(),
        Some(crate::diff::DiffResult::Text(_))
    );

    if has_text_diff {
        session.mutate_diff(|diff, tree| {
            invert_text_diff_staging(diff, tree, &session.path);
        });
    } else {
        crate::diff::staging::toggle_binary_file_staging_state(&session.tree, &session.path);
    }
}

fn check_hunk_contiguity(diff: &FileDiff, hunk_idx: usize) -> (bool, bool) {
    if diff.hunks.get(hunk_idx).is_none() {
        return (false, false);
    }
    let cp = hunk_idx > 0 && diff.hunks_contiguous(hunk_idx - 1, hunk_idx);
    let cn = hunk_idx + 1 < diff.hunks.len() && diff.hunks_contiguous(hunk_idx, hunk_idx + 1);
    (cp, cn)
}

fn handle_untoggle_single_line(
    diff: &mut FileDiff,
    tree: &RwLock<FileTree>,
    path: &PathBuf,
    hidx: usize,
    contiguous_prev: bool,
    contiguous_next: bool,
) {
    let default_staged = is_file_staged_default(tree, path);
    diff.ensure_selection_size(default_staged);

    let parent_is_staged = diff.parent_staging_status(hidx, default_staged);
    let start_idx = diff.hunk_start_idx(hidx);
    diff.set_hunk_staging(hidx, start_idx, parent_is_staged);
    crate::diff::staging::update_tree_staging_state(tree, path, diff, default_staged);

    let can_join_next = contiguous_next
        && hidx + 1 < diff.hunks.len()
        && diff.hunks[hidx + 1].marker == HunkMarker::None;
    let can_join_prev =
        contiguous_prev && hidx > 0 && diff.hunks[hidx - 1].marker != HunkMarker::LineToggle;

    if can_join_next {
        diff.join_hunk(hidx + 1, false, default_staged);
    }

    if can_join_prev {
        diff.join_hunk(hidx, false, default_staged);
    } else {
        diff.hunks[hidx].marker = HunkMarker::None;
    }
}

fn isolate_line_as_toggle_hunk(diff: &mut FileDiff, hidx: usize, line_within_hunk: usize) -> usize {
    let current_marker = diff.hunks[hidx].marker;
    let mut target_hunk_idx = hidx;

    if line_within_hunk > 0 {
        diff.split_hunk(target_hunk_idx, line_within_hunk, HunkMarker::None);
        target_hunk_idx += 1;
    }

    let remaining_len = diff
        .hunks
        .get(target_hunk_idx)
        .map(|h| h.lines.len())
        .unwrap_or(0);

    if remaining_len > 1 {
        let second_split_marker =
            if line_within_hunk == 0 && current_marker == HunkMarker::HunkSplit {
                HunkMarker::HunkSplit
            } else {
                HunkMarker::None
            };
        diff.split_hunk(target_hunk_idx, 1, second_split_marker);
    }

    diff.hunks[target_hunk_idx].marker = HunkMarker::LineToggle;
    target_hunk_idx
}

fn handle_boundary_split(
    diff: &mut FileDiff,
    tree: &RwLock<FileTree>,
    path: &PathBuf,
    hidx: usize,
) {
    if !diff.hunks_contiguous(hidx - 1, hidx) {
        return;
    }

    match diff.hunks[hidx].marker {
        HunkMarker::LineToggle | HunkMarker::None => {
            diff.hunks[hidx].marker = HunkMarker::HunkSplit;
        }
        HunkMarker::HunkSplit => {
            resolve_split_marker_join(diff, tree, path, hidx);
        }
    }
}

fn resolve_split_marker_join(
    diff: &mut FileDiff,
    tree: &RwLock<FileTree>,
    path: &PathBuf,
    hidx: usize,
) {
    if diff.hunks[hidx - 1].marker != HunkMarker::LineToggle {
        diff.join_hunk(hidx, true, is_file_staged_default(tree, path));
        return;
    }

    let default_staged = is_file_staged_default(tree, path);
    diff.ensure_selection_size(default_staged);

    let prev_is_staged = diff.parent_staging_status(hidx, default_staged);
    diff.hunks[hidx].marker = HunkMarker::None;
    diff.sync_contiguous_to_parent(hidx, prev_is_staged);
    crate::diff::staging::update_tree_staging_state(tree, path, diff, default_staged);
}

#[cfg(test)]
mod tests {
    use super::super::staging_session::StagingSession;
    use super::*;
    use crate::app::UiState;
    use crate::diff::{ConflictedFile, DiffLine, DiffResult, FileDiff, Hunk, LineSelections, Side};
    use crate::diff_cache::DiffCache;
    use crate::tree::FileTree;
    use parking_lot::{Mutex, RwLock};
    use ratatui::widgets::TableState;
    use std::path::PathBuf;
    use std::sync::Arc;

    const NEW: &str = "\
fn main() {
<<<<<<< ours
    let value = 1;
||||||| base
    let value = 0;
=======
    let value = 2;
>>>>>>> theirs
    println!(\"value = {value}\");
}
";
    const OLD: &str = "\
fn main() {
    let value = 0;
    println!(\"value = {value}\");
}
";

    /// Single hunk covering the whole marker block: context + 7 additions.
    fn conflict_diff() -> FileDiff {
        let mut lines = vec![DiffLine::Context {
            new_line_idx: 0,
            old_line_idx: 0,
        }];
        for new_line_idx in 1..=7 {
            lines.push(DiffLine::Addition {
                new_line_idx,
                inline_highlights: Vec::new(),
            });
        }
        lines.push(DiffLine::Context {
            new_line_idx: 8,
            old_line_idx: 2,
        });
        lines.push(DiffLine::Context {
            new_line_idx: 9,
            old_line_idx: 3,
        });
        FileDiff {
            old_file_content: Arc::from(OLD),
            new_file_content: Arc::from(NEW),
            hunks: vec![Hunk {
                before_lines: 0..4,
                after_lines: 0..10,
                lines,
                marker: Default::default(),
            }],
            line_selections: LineSelections::default(),
        }
    }

    fn harness(selected: usize, offset: usize) -> (StagingSession, Arc<Mutex<UiState>>, PathBuf) {
        let path = PathBuf::from("merged.txt");
        let cache = DiffCache::default();
        cache.diffs.set(
            path.clone(),
            Arc::new(DiffResult::Text(conflict_diff())),
            cache.diffs.generation(),
        );
        let ui = Arc::new(Mutex::new(UiState::new(false)));
        {
            let mut guard = ui.lock();
            guard.file_view.current_path = Some(path.clone());
            guard.resolve = Some(crate::app::ui_state::ResolveState::new(
                ConflictedFile::parse(NEW).expect("conflicted"),
            ));
            let st = guard
                .file_view
                .scroll_states
                .entry(path.clone())
                .or_insert_with(TableState::default);
            st.select(Some(selected));
            *st.offset_mut() = offset;
        }
        let tree = Arc::new(RwLock::new(FileTree::default()));
        let session = StagingSession::try_new(tree, cache, ui.clone(), crate::app::Operation::Diff).expect("session");
        (session, ui, path)
    }

    fn cursor(ui: &Arc<Mutex<UiState>>, path: &PathBuf) -> (Option<usize>, usize) {
        let guard = ui.lock();
        let st = guard.file_view.scroll_states.get(path).expect("scroll");
        (st.selected(), st.offset())
    }

    /// Space on the `=======` separator selects theirs (the side below it),
    /// folds the conflict to a frame, and anchors the cursor: it was on a
    /// hidden line, so it lands on the header with the view shifted to keep
    /// it as close as possible to its screen position.
    #[test]
    fn separator_selects_theirs_and_anchors_to_header() {
        let (session, ui, path) = harness(5, 3);
        assert!(toggle_conflict_at_cursor(&session));

        let guard = ui.lock();
        let state = guard.resolve.as_ref().expect("resolve");
        assert_eq!(state.choice(0), Some(Side::Theirs));
        assert!(state.is_folded(0));
        drop(guard);

        // Header is row 1; screen position was 5 - 3 = 2, clamped to 1 - 0.
        assert_eq!(cursor(&ui, &path), (Some(1), 0));
    }

    /// Space on a folded kept (center) line unfolds, like space on the frame:
    /// conflict hunks are not stageable, so there is nothing to toggle.
    #[test]
    fn kept_line_space_unfolds() {
        let (session, ui, path) = harness(6, 4);
        assert!(toggle_conflict_at_cursor(&session));
        assert_eq!(cursor(&ui, &path), (Some(2), 0));

        // Space on the kept center line unfolds back to the markers,
        // anchored to the range start.
        let session2 =
            StagingSession::try_new(session.tree.clone(), session.cache.clone(), ui.clone(), session.operation)
                .expect("session");
        assert!(toggle_conflict_at_cursor(&session2));
        let guard = ui.lock();
        let state = guard.resolve.as_ref().expect("resolve");
        assert_eq!(state.choice(0), None);
        assert!(!state.is_folded(0));
        drop(guard);
        assert_eq!(cursor(&ui, &path), (Some(1), 0));
    }

    /// Folding clears stale hunk markers: a `t`-split hunk overlapping the
    /// conflict would otherwise keep its `▶` glyph on the kept lines, even
    /// though hunk markers are meaningless inside conflicts.
    #[test]
    fn folding_clears_stale_hunk_markers() {
        let (session, ui, path) = harness(6, 4);
        session.cache.diffs.update(&path, |d| {
            if let crate::diff::DiffResult::Text(t) = d {
                t.hunks[0].marker = crate::diff::HunkMarker::LineToggle;
            }
        });
        assert!(toggle_conflict_at_cursor(&session));

        let cached = session.cache.diffs.get(&path).expect("diff");
        let crate::diff::DiffResult::Text(diff) = cached.as_ref() else {
            panic!("text diff");
        };
        assert_eq!(diff.hunks[0].marker, crate::diff::HunkMarker::None);

        let guard = ui.lock();
        assert_eq!(
            guard.resolve.as_ref().expect("resolve").choice(0),
            Some(Side::Theirs)
        );
    }

    /// `t` and `s` are no-ops inside conflicts: conflict hunks are not
    /// stageable, so per-line ops must leave them alone.
    #[test]
    fn line_toggle_and_split_ignore_conflict_lines() {
        let (session, ui, path) = harness(2, 0);
        toggle_single_line_at_cursor(&session);
        split_hunk_at_cursor(&session);

        let guard = ui.lock();
        assert_eq!(
            guard.resolve.as_ref().expect("resolve").choice(0),
            None,
            "no side chosen"
        );
        drop(guard);

        let cached = session.cache.diffs.get(&path).expect("diff");
        let crate::diff::DiffResult::Text(diff) = cached.as_ref() else {
            panic!("text diff");
        };
        assert!(diff.line_selections.is_empty(), "no selection materialized");
        assert_eq!(diff.hunks.len(), 1, "no split performed");
    }

    /// Space on the folded header unfolds and re-shows the markers, keeping
    /// the cursor anchored to the same screen position.
    #[test]
    fn header_space_unfolds_with_anchor() {
        let (session, ui, path) = harness(6, 4);
        assert!(toggle_conflict_at_cursor(&session));
        assert_eq!(cursor(&ui, &path), (Some(2), 0));

        // Move to the header row and unfold.
        {
            let mut guard = ui.lock();
            let st = guard
                .file_view
                .scroll_states
                .get_mut(&path)
                .expect("scroll");
            st.select(Some(1));
        }
        let tree = Arc::new(RwLock::new(FileTree::default()));
        let cache = DiffCache::default();
        cache.diffs.set(
            path.clone(),
            Arc::new(DiffResult::Text(conflict_diff())),
            cache.diffs.generation(),
        );
        // NOTE: the live diff (with staging selections) lives in the first
        // harness cache; rebuild the session against the same ui so the
        // cursor maps through the folded layout. Selections are untouched
        // by conflict folds, so rebuilding the cache is loss-free here.
        let session2 = StagingSession::try_new(tree, cache, ui.clone(), crate::app::Operation::Diff).expect("session");
        assert!(toggle_conflict_at_cursor(&session2));

        let guard = ui.lock();
        let state = guard.resolve.as_ref().expect("resolve");
        assert_eq!(state.choice(0), None);
        assert!(!state.is_folded(0));
        drop(guard);

        // `<<<<<<<` is back at row 1; screen position 1 - 0 is preserved.
        assert_eq!(cursor(&ui, &path), (Some(1), 0));
    }
}
