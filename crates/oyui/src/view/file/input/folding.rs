use super::super::FileViewData;
use crate::diff_cache::DiffCache;
use std::path::PathBuf;

pub fn handle_folding(
    data: &mut FileViewData,
    path: &PathBuf,
    current_selected: usize,
    screen_y: usize,
    cache: &DiffCache,
) -> (usize, Option<usize>) {
    let mut target_logical = 0;
    if let Some(mapping) = data.line_mapping.get(path) {
        target_logical = mapping.get(current_selected).copied().unwrap_or(0);
    }

    data.is_folded = !data.is_folded;

    let next_selected =
        if let Some(crate::diff::DiffResult::Text(diff)) = cache.diffs.get(path).value() {
            let new_lines_len = diff.new_text.lines().count();
            let new_map = data.get_line_map(diff, new_lines_len);

            new_map
                .iter()
                .position(|&l| l >= target_logical)
                .unwrap_or(new_map.len().saturating_sub(1))
        } else {
            0
        };

    let next_offset = Some(next_selected.saturating_sub(screen_y));

    (next_selected, next_offset)
}
