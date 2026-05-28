use super::super::FileViewData;
use crate::diff_cache::DiffCache;
use std::path::PathBuf;

pub fn handle_hscroll(data: &mut FileViewData, path: &PathBuf, delta: isize, cache: &DiffCache) {
    let mut max_line_len = 0;

    if let Some(crate::diff::DiffResult::Text(diff)) = cache.diffs.get(path).value() {
        let old_max = diff
            .old_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let new_max = diff
            .new_text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        max_line_len = old_max.max(new_max);
    }

    let code_col_width = data.last_width.saturating_sub(6);
    let max_hscroll = max_line_len.saturating_sub(code_col_width) + 10;

    let hs = data.hscroll_states.entry(path.clone()).or_insert(0);
    *hs = (*hs as isize + delta).clamp(0, max_hscroll as isize) as usize;
}
