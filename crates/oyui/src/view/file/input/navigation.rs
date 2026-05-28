use super::super::FileViewData;
use std::path::PathBuf;

pub fn next_hunk(
    data: &FileViewData,
    path: &PathBuf,
    current_selected: usize,
    last_height: usize,
) -> (usize, Option<usize>) {
    if let Some(starts) = data.hunk_starts.get(path) {
        let target = starts
            .iter()
            .find(|&&idx| idx > current_selected)
            .or_else(|| starts.first());

        if let Some(&t) = target {
            let padding = last_height.saturating_sub(1) / 3;
            return (t, Some(t.saturating_sub(padding)));
        }
    }
    (current_selected, None)
}

pub fn prev_hunk(
    data: &FileViewData,
    path: &PathBuf,
    current_selected: usize,
    last_height: usize,
) -> (usize, Option<usize>) {
    if let Some(starts) = data.hunk_starts.get(path) {
        let target = starts
            .iter()
            .rev()
            .find(|&&idx| idx < current_selected)
            .or_else(|| starts.last());

        if let Some(&t) = target {
            let padding = last_height.saturating_sub(1) / 3;
            return (t, Some(t.saturating_sub(padding)));
        }
    }
    (current_selected, None)
}
