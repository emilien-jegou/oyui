use super::super::ViewAction;
use super::FileViewData;
use crate::diff_cache::DiffCache;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl FileViewData {
    #[tracing::instrument(skip_all)]
    pub fn handle_input(&mut self, key: KeyEvent, cache: &DiffCache) -> ViewAction {
        let mut clear_pending = true;
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        if let Some(path) = &self.current_path {
            let max_idx = self
                .row_counts
                .get(path)
                .map(|&c| c.saturating_sub(1))
                .unwrap_or(0);

            let (current_selected, current_offset) = {
                let s = self.scroll_states.get(path);
                (
                    s.and_then(|st| st.selected()).unwrap_or(0),
                    s.map(|st| st.offset()).unwrap_or(0),
                )
            };
            let screen_y = current_selected.saturating_sub(current_offset);

            let mut next_selected = current_selected;
            let mut next_offset = None;

            let mut move_cursor = |delta: isize| {
                next_selected =
                    (current_selected as isize + delta).clamp(0, max_idx as isize) as usize;
            };

            let mut move_hscroll = |delta: isize| {
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

                let code_col_width = self.last_width.saturating_sub(6);
                let max_hscroll = max_line_len.saturating_sub(code_col_width) + 10;

                let hs = self.hscroll_states.entry(path.clone()).or_insert(0);
                *hs = (*hs as isize + delta).clamp(0, max_hscroll as isize) as usize;
            };

            match (key.code, is_ctrl) {
                (KeyCode::Enter, _) => return ViewAction::ConfirmMerge,
                (KeyCode::Char('c'), true) => return ViewAction::QuitWithAbort,
                (KeyCode::Char('j'), true) => move_cursor(5),
                (KeyCode::Char('k'), true) => move_cursor(-5),
                (KeyCode::Char('d'), true) => move_cursor(20),
                (KeyCode::Char('u'), true) => move_cursor(-20),

                (KeyCode::Char('q'), false) => return ViewAction::QuitWithAbort,
                (KeyCode::Esc, _) | (KeyCode::Char('h'), false) => {
                    return ViewAction::CloseFileView
                }

                (KeyCode::Char('l'), true) | (KeyCode::Right, _) => move_hscroll(4),
                (KeyCode::Char('h'), true) | (KeyCode::Left, _) => move_hscroll(-4),

                (KeyCode::Char('j'), false) | (KeyCode::Down, _) => move_cursor(1),
                (KeyCode::Char('k'), false) | (KeyCode::Up, _) => move_cursor(-1),
                (KeyCode::Char('n'), false) => {
                    if let Some(starts) = self.hunk_starts.get(path) {
                        let target = starts
                            .iter()
                            .find(|&&idx| idx > current_selected)
                            .or_else(|| starts.first());
                        if let Some(&t) = target {
                            next_selected = t;
                            let padding = self.last_height.saturating_sub(1) / 3;
                            next_offset = Some(t.saturating_sub(padding));
                        }
                    }
                }
                (KeyCode::Char('N'), false) => {
                    if let Some(starts) = self.hunk_starts.get(path) {
                        let target = starts
                            .iter()
                            .rev()
                            .find(|&&idx| idx < current_selected)
                            .or_else(|| starts.last());
                        if let Some(&t) = target {
                            next_selected = t;
                            let padding = self.last_height.saturating_sub(1) / 3;
                            next_offset = Some(t.saturating_sub(padding));
                        }
                    }
                }
                (KeyCode::Char(' '), false) => {
                    let mut target_hunk = None;
                    let mut has_any_hunks = false;

                    if let Some(mapping) = self.row_to_hunk.get(path) {
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
                        return ViewAction::ToggleStageHunk(hunk_idx);
                    } else if !has_any_hunks {
                        // Fallback only if the file literally contains zero hunks
                        return ViewAction::ToggleStageSelected;
                    } else {
                        // We have hunks, but cursor is too far from any of them. Space does nothing.
                        return ViewAction::None;
                    }
                }

                (KeyCode::Char('G'), false) => next_selected = max_idx,
                (KeyCode::Char('z'), false) => {
                    let mut target_logical = 0;
                    if let Some(mapping) = self.line_mapping.get(path) {
                        target_logical = mapping.get(current_selected).copied().unwrap_or(0);
                    }

                    self.is_folded = !self.is_folded;

                    if let Some(crate::diff::DiffResult::Text(diff)) = cache.diffs.get(path).value()
                    {
                        let new_lines_len = diff.new_text.lines().count();
                        let new_map = self.get_line_map(diff, new_lines_len);

                        next_selected = new_map
                            .iter()
                            .position(|&l| l >= target_logical)
                            .unwrap_or(new_map.len().saturating_sub(1));
                    } else {
                        next_selected = 0;
                    }

                    next_offset = Some(next_selected.saturating_sub(screen_y));
                }
                (KeyCode::Char('g'), false) => {
                    if self.pending_g {
                        next_selected = 0;
                        self.pending_g = false;
                        clear_pending = false;
                    } else {
                        self.pending_g = true;
                        clear_pending = false;
                    }
                }
                _ => {}
            }

            let state = self.scroll_states.entry(path.clone()).or_default();
            state.select(Some(next_selected));
            if let Some(off) = next_offset {
                *state.offset_mut() = off;
            }
        } else {
            // No current path, handle globals
            match (key.code, is_ctrl) {
                (KeyCode::Char('c'), true) | (KeyCode::Char('q'), false) => {
                    return ViewAction::QuitWithAbort
                }
                (KeyCode::Esc, _) | (KeyCode::Char('h'), false) => {
                    return ViewAction::CloseFileView
                }
                _ => {}
            }
        }

        if clear_pending {
            self.pending_g = false;
        }

        ViewAction::None
    }
}
