pub mod context;
pub mod folding;
pub mod handlers;
pub mod navigation;
pub mod scroll;
pub mod staging;

use super::super::ViewAction;
use super::FileViewData;
use crate::diff_cache::DiffCache;
use context::InputContext;
use std::path::PathBuf;

impl FileViewData {
    /// Safe boundary for external routing to act on context and capture resulting offsets.
    #[tracing::instrument(skip_all)]
    pub fn apply_input<F>(
        &mut self,
        path: &PathBuf,
        cache: &DiffCache,
        mut f: F,
    ) -> (Option<ViewAction>, bool)
    where
        F: FnMut(&mut InputContext) -> (Option<ViewAction>, bool),
    {
        let max_idx = self
            .row_counts
            .get(path)
            .map(|&c| c.saturating_sub(1))
            .unwrap_or(0);

        let (current_row_idx, current_offset) = {
            let s = self.scroll_states.get(path);
            (
                s.and_then(|st| st.selected()).unwrap_or(0),
                s.map(|st| st.offset()).unwrap_or(0),
            )
        };
        let cursor_screen_offset = current_row_idx.saturating_sub(current_offset);

        // Fixed Borrow Checker: execute closure and isolate the context borrow.
        let (target_row, target_offset, result) = {
            let mut ctx = InputContext {
                data: self,
                cache,
                path,
                max_idx,
                current_row_idx,
                cursor_screen_offset,
                target_row_idx: current_row_idx,
                target_scroll_offset: None,
                clear_pending: true,
            };

            let result = f(&mut ctx);

            if ctx.clear_pending {
                ctx.data.pending_g = false;
            }

            (ctx.target_row_idx, ctx.target_scroll_offset, result)
        };

        // Persist local cursor modifications safely.
        let state = self.scroll_states.entry(path.clone()).or_default();
        state.select(Some(target_row));
        if let Some(off) = target_offset {
            *state.offset_mut() = off;
        }

        result
    }
}
