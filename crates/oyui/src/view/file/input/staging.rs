use super::super::super::ViewAction;
use super::super::FileViewData;
use std::path::PathBuf;

pub fn handle_staging(data: &FileViewData, path: &PathBuf, current_selected: usize) -> ViewAction {
    let mut target_hunk = None;
    let mut has_any_hunks = false;

    if let Some(mapping) = data.row_to_hunk.get(path) {
        // First check if we are directly hovering over a mapped diff line
        if let Some(&Some(hunk_idx)) = mapping.get(current_selected) {
            target_hunk = Some(hunk_idx);
            has_any_hunks = true;
        } else {
            // If we are over an unmapped space, find visually closest
            let mut closest_dist = usize::MAX;
            for (idx, &hunk_opt) in mapping.iter().enumerate() {
                if let Some(h) = hunk_opt {
                    has_any_hunks = true;
                    let dist = current_selected.abs_diff(idx);
                    if dist < closest_dist {
                        closest_dist = dist;
                        target_hunk = Some(h);
                    }
                }
            }

            if closest_dist > 4 {
                target_hunk = None;
            }
        }
    }

    if let Some(hunk_idx) = target_hunk {
        ViewAction::ToggleStageHunk(hunk_idx)
    } else if !has_any_hunks {
        ViewAction::ToggleStageSelected
    } else {
        ViewAction::None
    }
}
