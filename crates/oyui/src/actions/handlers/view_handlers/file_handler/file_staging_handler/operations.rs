use super::hunk_mutations::{join_hunk_at, split_hunk_at};
use super::staging_session::StagingSession;
use super::staging_sync::*;
use super::utils::*;
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
    let view_read = session.view.read();
    let mappings = view_read.row_to_hunk.get(&session.path);
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
    drop(view_read);

    if let Some(hunk_idx) = hidx {
        let is_line_toggle = {
            if let Some(crate::diff::DiffResult::Text(diff)) =
                session.cache.diffs.get(&session.path).value()
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
            split_hunk_at(diff, hidx, split_idx, HunkMarker::HunkSplit);
        }
    });
}

pub fn invert_staging(session: &StagingSession) {
    let has_text_diff = matches!(
        session.cache.diffs.get(&session.path).value(),
        Some(crate::diff::DiffResult::Text(_))
    );

    if has_text_diff {
        session.mutate_diff(|diff, tree| {
            invert_text_diff_staging(diff, tree, &session.path);
        });
    } else {
        toggle_binary_file_staging_state(&session.tree, &session.path);
    }
}

fn check_hunk_contiguity(diff: &FileDiff, hunk_idx: usize) -> (bool, bool) {
    if diff.hunks.get(hunk_idx).is_none() {
        return (false, false);
    }
    let cp = hunk_idx > 0 && are_hunks_contiguous(diff, hunk_idx - 1, hunk_idx);
    let cn = hunk_idx + 1 < diff.hunks.len() && are_hunks_contiguous(diff, hunk_idx, hunk_idx + 1);
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
    ensure_selection_size(diff, default_staged);

    let parent_is_staged = find_parent_staging_status(diff, hidx, default_staged);
    let start_idx = get_hunk_start_line_idx(diff, hidx);
    set_hunk_line_staging(diff, hidx, start_idx, parent_is_staged);
    update_tree_staging_state(tree, path, diff, default_staged);

    let can_join_next = contiguous_next
        && hidx + 1 < diff.hunks.len()
        && diff.hunks[hidx + 1].marker == HunkMarker::None;
    let can_join_prev =
        contiguous_prev && hidx > 0 && diff.hunks[hidx - 1].marker != HunkMarker::LineToggle;

    if can_join_next {
        join_hunk_at(diff, tree, path, hidx + 1, false);
    }

    if can_join_prev {
        join_hunk_at(diff, tree, path, hidx, false);
    } else {
        diff.hunks[hidx].marker = HunkMarker::None;
    }
}

fn isolate_line_as_toggle_hunk(diff: &mut FileDiff, hidx: usize, line_within_hunk: usize) -> usize {
    let current_marker = diff.hunks[hidx].marker;
    let mut target_hunk_idx = hidx;

    if line_within_hunk > 0 {
        split_hunk_at(diff, target_hunk_idx, line_within_hunk, HunkMarker::None);
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
        split_hunk_at(diff, target_hunk_idx, 1, second_split_marker);
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
    if !are_hunks_contiguous(diff, hidx - 1, hidx) {
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
        join_hunk_at(diff, tree, path, hidx, true);
        return;
    }

    let default_staged = is_file_staged_default(tree, path);
    ensure_selection_size(diff, default_staged);

    let prev_is_staged = find_parent_staging_status(diff, hidx, default_staged);
    diff.hunks[hidx].marker = HunkMarker::None;
    sync_contiguous_lines_to_parent_status(diff, hidx, prev_is_staged);
    update_tree_staging_state(tree, path, diff, default_staged);
}
