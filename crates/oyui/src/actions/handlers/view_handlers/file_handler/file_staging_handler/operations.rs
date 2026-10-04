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

pub fn toggle_stage_at_cursor(session: &StagingSession) {
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

pub fn toggle_single_line_at_cursor(session: &StagingSession) {
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
