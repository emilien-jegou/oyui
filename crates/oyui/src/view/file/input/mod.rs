pub mod context;
pub mod folding;
pub mod handlers;
pub mod keybinds;
pub mod navigation;
pub mod scroll;
pub mod staging;

use super::super::ViewAction;
use super::FileViewData;
use crate::diff_cache::DiffCache;
use context::{InputContext, KeybindRegistry};
use crossterm::event::{KeyCode, KeyEvent};
use handlers::{
    CursorKeybinds, FoldingKeybinds, GlobalKeybinds, NavigationKeybinds, ScrollKeybinds,
    StagingKeybinds,
};
use keybinds::Keybinds;

impl FileViewData {
    #[tracing::instrument(skip_all)]
    pub fn handle_input(&mut self, key: KeyEvent, cache: &DiffCache) -> ViewAction {
        let global_keybinds = GlobalKeybinds::builder()
            .confirm(Keybinds::code(KeyCode::Enter))
            .quit(Keybinds::char('q').with_ctrl('c'))
            .close(Keybinds::char('h').with_code(KeyCode::Esc))
            .build();

        // Fallback for global context when no file is active
        if self.current_path.is_none() {
            if global_keybinds.quit.matches(&key) {
                return ViewAction::QuitWithAbort;
            }
            if global_keybinds.close.matches(&key) {
                return ViewAction::CloseFileView;
            }
            return ViewAction::None;
        }

        let path = self.current_path.clone().unwrap();
        let max_idx = self
            .row_counts
            .get(&path)
            .map(|&c| c.saturating_sub(1))
            .unwrap_or(0);

        let (current_row_idx, current_offset) = {
            let s = self.scroll_states.get(&path);
            (
                s.and_then(|st| st.selected()).unwrap_or(0),
                s.map(|st| st.offset()).unwrap_or(0),
            )
        };
        let cursor_screen_offset = current_row_idx.saturating_sub(current_offset);

        let (action, target_row_idx, target_scroll_offset) = {
            let mut ctx = InputContext {
                data: self,
                cache,
                path: &path,
                max_idx,
                current_row_idx,
                cursor_screen_offset,
                target_row_idx: current_row_idx,
                target_scroll_offset: None,
                clear_pending: true,
            };

            let scroll_keybinds = ScrollKeybinds::builder()
                .left(Keybinds::code(KeyCode::Left).with_ctrl('h'))
                .right(Keybinds::code(KeyCode::Right).with_ctrl('l'))
                .build();

            let cursor_keybinds = CursorKeybinds::builder()
                .down(Keybinds::char('j').with_code(KeyCode::Down))
                .up(Keybinds::char('k').with_code(KeyCode::Up))
                .half_page_down(Keybinds::new().with_ctrl('d'))
                .half_page_up(Keybinds::new().with_ctrl('u'))
                .go_bottom(Keybinds::char('G'))
                .go_top(Keybinds::char('g'))
                .build();

            let nav_keybinds = NavigationKeybinds::builder()
                .next_hunk(Keybinds::char('n'))
                .prev_hunk(Keybinds::char('N'))
                .build();

            let staging_keybinds = StagingKeybinds::builder()
                .toggle_stage(Keybinds::char(' '))
                .build();

            let folding_keybinds = FoldingKeybinds::builder()
                .toggle_fold(Keybinds::char('z'))
                .build();

            let executed_action = KeybindRegistry::new(&mut ctx, key)
                .process(&global_keybinds)
                .process(&scroll_keybinds)
                .process(&cursor_keybinds)
                .process(&nav_keybinds)
                .process(&staging_keybinds)
                .process(&folding_keybinds)
                .action();

            if ctx.clear_pending {
                ctx.data.pending_g = false;
            }

            (
                executed_action,
                ctx.target_row_idx,
                ctx.target_scroll_offset,
            )
        };

        // Persist local cursor modifications back to the state manager
        let state = self.scroll_states.entry(path).or_default();
        state.select(Some(target_row_idx));
        if let Some(off) = target_scroll_offset {
            *state.offset_mut() = off;
        }

        action
    }
}
