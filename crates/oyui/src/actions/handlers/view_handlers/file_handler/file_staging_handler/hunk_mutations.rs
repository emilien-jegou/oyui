use super::utils::*;
use crate::diff::{DiffLine, FileDiff, HunkMarker};
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::path::PathBuf;

pub fn split_hunk_at(diff: &mut FileDiff, hunk_idx: usize, split_idx: usize, marker: HunkMarker) {
    if hunk_idx >= diff.hunks.len() {
        return;
    }
    let hunk = &diff.hunks[hunk_idx];
    if split_idx == 0 || split_idx >= hunk.lines.len() {
        return;
    }

    let mut hunk_a = hunk.clone();
    let mut hunk_b = hunk.clone();

    hunk_a.lines = hunk.lines[..split_idx].to_vec();
    hunk_b.lines = hunk.lines[split_idx..].to_vec();

    let (old_end, new_end) = calculate_hunk_ranges(&hunk_a);

    hunk_a.after_lines = hunk.after_lines.start..new_end;
    hunk_a.before_lines = hunk.before_lines.start..old_end;

    hunk_b.after_lines = new_end..hunk.after_lines.end;
    hunk_b.before_lines = old_end..hunk.before_lines.end;
    hunk_b.marker = marker;

    diff.hunks.remove(hunk_idx);
    diff.hunks.insert(hunk_idx, hunk_b);
    diff.hunks.insert(hunk_idx, hunk_a);
}

fn calculate_hunk_ranges(hunk: &crate::diff::Hunk) -> (usize, usize) {
    let mut old_idx = hunk.before_lines.start;
    let mut new_idx = hunk.after_lines.start;
    for line in &hunk.lines {
        match line {
            DiffLine::Context { .. } => {
                old_idx += 1;
                new_idx += 1;
            }
            DiffLine::Deletion { .. } => {
                old_idx += 1;
            }
            DiffLine::Addition { .. } => {
                new_idx += 1;
            }
        }
    }
    (old_idx, new_idx)
}

pub fn join_hunk_at(
    diff: &mut FileDiff,
    tree_rw: &RwLock<FileTree>,
    path: &PathBuf,
    hunk_idx: usize,
    sync_staging_before_merge: bool,
) {
    if hunk_idx == 0 || hunk_idx >= diff.hunks.len() {
        return;
    }

    if !are_hunks_contiguous(diff, hunk_idx - 1, hunk_idx) {
        return;
    }

    if sync_staging_before_merge {
        let default_staged = is_file_staged_default(tree_rw, path);
        ensure_selection_size(diff, default_staged);
        let parent_status = find_parent_staging_status(diff, hunk_idx, default_staged);
        sync_contiguous_lines_to_parent_status(diff, hunk_idx, parent_status);
        super::staging_sync::update_tree_staging_state(tree_rw, path, diff, default_staged);
    }

    perform_merge(diff, hunk_idx);
}

fn perform_merge(diff: &mut FileDiff, hunk_idx: usize) {
    let prev_hunk = diff.hunks[hunk_idx - 1].clone();
    let curr_hunk = diff.hunks[hunk_idx].clone();

    let mut merged = prev_hunk.clone();
    merged.marker = if prev_hunk.marker == HunkMarker::LineToggle {
        curr_hunk.marker
    } else {
        prev_hunk.marker
    };
    merged.lines.extend(curr_hunk.lines);
    merged.before_lines = prev_hunk.before_lines.start..curr_hunk.before_lines.end;
    merged.after_lines = prev_hunk.after_lines.start..curr_hunk.after_lines.end;

    diff.hunks.remove(hunk_idx);
    diff.hunks.remove(hunk_idx - 1);
    diff.hunks.insert(hunk_idx - 1, merged);
}
