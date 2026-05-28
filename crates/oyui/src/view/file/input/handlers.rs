use super::context::{
    CursorOperations, FoldingOperations, NavigationOperations, ScrollOperations, StagingOperations,
};
use crate::commons::input::{KeybindHandler, KeybindMatcher, Keybinds};
use crate::view::ViewAction;
use typed_builder::TypedBuilder;

#[derive(TypedBuilder)]
pub struct GlobalKeybinds {
    pub confirm: Keybinds,
    pub quit: Keybinds,
    pub close: Keybinds,
}

impl<C> KeybindHandler<C, ViewAction> for GlobalKeybinds {
    fn handle(&self, _ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches_action(&self.confirm, |a| a.set_action(ViewAction::ConfirmMerge));
        matcher.matches_action(&self.quit, |a| a.set_action(ViewAction::QuitWithAbort));
        matcher.matches_action(&self.close, |a| a.set_action(ViewAction::CloseFileView));
    }
}

#[derive(TypedBuilder)]
pub struct CursorKeybinds {
    pub down: Keybinds,
    pub up: Keybinds,
    pub half_page_down: Keybinds,
    pub half_page_up: Keybinds,
    pub go_bottom: Keybinds,
    pub go_top: Keybinds,
}

impl<C: CursorOperations> KeybindHandler<C, ViewAction> for CursorKeybinds {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches(&self.down, || ctx.move_cursor_by(1));
        matcher.matches(&self.up, || ctx.move_cursor_by(-1));
        matcher.matches(&self.half_page_down, || ctx.move_cursor_by(20));
        matcher.matches(&self.half_page_up, || ctx.move_cursor_by(-20));

        matcher.matches(&self.go_bottom, || ctx.move_cursor_to_bottom());
        matcher.matches(&self.go_top, || ctx.handle_g_sequence());
    }
}

#[derive(TypedBuilder)]
pub struct ScrollKeybinds {
    pub left: Keybinds,
    pub right: Keybinds,
}

impl<C: ScrollOperations> KeybindHandler<C, ViewAction> for ScrollKeybinds {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches(&self.right, || ctx.scroll_right());
        matcher.matches(&self.left, || ctx.scroll_left());
    }
}

#[derive(TypedBuilder)]
pub struct NavigationKeybinds {
    pub next_hunk: Keybinds,
    pub prev_hunk: Keybinds,
}

impl<C: NavigationOperations> KeybindHandler<C, ViewAction> for NavigationKeybinds {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches(&self.next_hunk, || ctx.next_hunk());
        matcher.matches(&self.prev_hunk, || ctx.prev_hunk());
    }
}

#[derive(TypedBuilder)]
pub struct StagingKeybinds {
    pub toggle_stage: Keybinds,
}

impl<C: StagingOperations> KeybindHandler<C, ViewAction> for StagingKeybinds {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches_action(&self.toggle_stage, |a| {
            a.set_action(ctx.toggle_stage());
        });
    }
}

#[derive(TypedBuilder)]
pub struct FoldingKeybinds {
    pub toggle_fold: Keybinds,
}

impl<C: FoldingOperations> KeybindHandler<C, ViewAction> for FoldingKeybinds {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher<ViewAction>) {
        matcher.matches(&self.toggle_fold, || ctx.toggle_fold());
    }
}
