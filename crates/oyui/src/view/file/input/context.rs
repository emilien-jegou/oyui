use super::super::FileViewData;
use super::keybinds::Keybinds;
use crate::diff_cache::DiffCache;
use crate::ViewAction;
use crossterm::event::KeyEvent;
use std::path::PathBuf;

/// The shared context passed down to all keybind handlers.
pub struct InputContext<'a, 'ctx> {
    pub data: &'a mut FileViewData,
    pub cache: &'ctx DiffCache,
    pub path: &'ctx PathBuf,
    pub max_idx: usize,
    pub current_row_idx: usize,
    pub cursor_screen_offset: usize,
    pub target_row_idx: usize,
    pub target_scroll_offset: Option<usize>,
    pub clear_pending: bool,
}

// --- Capability Traits ---

pub trait CursorOperations {
    fn move_cursor_by(&mut self, delta: isize);
    fn move_cursor_to_bottom(&mut self);
    fn handle_g_sequence(&mut self);
}

pub trait ScrollOperations {
    fn scroll_right(&mut self);
    fn scroll_left(&mut self);
}

pub trait NavigationOperations {
    fn next_hunk(&mut self);
    fn prev_hunk(&mut self);
}

pub trait StagingOperations {
    fn toggle_stage(&mut self) -> ViewAction;
}

pub trait FoldingOperations {
    fn toggle_fold(&mut self);
}

// --- Implementations for InputContext ---

impl CursorOperations for InputContext<'_, '_> {
    fn move_cursor_by(&mut self, delta: isize) {
        self.target_row_idx =
            (self.current_row_idx as isize + delta).clamp(0, self.max_idx as isize) as usize;
    }

    fn move_cursor_to_bottom(&mut self) {
        self.target_row_idx = self.max_idx;
    }

    fn handle_g_sequence(&mut self) {
        if self.data.pending_g {
            self.target_row_idx = 0;
            self.data.pending_g = false;
            self.clear_pending = false;
        } else {
            self.data.pending_g = true;
            self.clear_pending = false;
        }
    }
}

impl ScrollOperations for InputContext<'_, '_> {
    fn scroll_right(&mut self) {
        super::scroll::handle_hscroll(self.data, self.path, 4, self.cache);
    }

    fn scroll_left(&mut self) {
        super::scroll::handle_hscroll(self.data, self.path, -4, self.cache);
    }
}

impl NavigationOperations for InputContext<'_, '_> {
    fn next_hunk(&mut self) {
        let (sel, off) = super::navigation::next_hunk(
            self.data,
            self.path,
            self.current_row_idx,
            self.data.last_height,
        );
        self.target_row_idx = sel;
        if off.is_some() {
            self.target_scroll_offset = off;
        }
    }

    fn prev_hunk(&mut self) {
        let (sel, off) = super::navigation::prev_hunk(
            self.data,
            self.path,
            self.current_row_idx,
            self.data.last_height,
        );
        self.target_row_idx = sel;
        if off.is_some() {
            self.target_scroll_offset = off;
        }
    }
}

impl StagingOperations for InputContext<'_, '_> {
    fn toggle_stage(&mut self) -> ViewAction {
        super::staging::handle_staging(self.data, self.path, self.current_row_idx)
    }
}

impl FoldingOperations for InputContext<'_, '_> {
    fn toggle_fold(&mut self) {
        let (sel, off) = super::folding::handle_folding(
            self.data,
            self.path,
            self.current_row_idx,
            self.cursor_screen_offset,
            self.cache,
        );
        self.target_row_idx = sel;
        self.target_scroll_offset = off;
    }
}

// --- Dispatch & Matcher Mechanics ---

pub struct ActionSetter<'a> {
    action: &'a mut Option<ViewAction>,
}

impl ActionSetter<'_> {
    pub fn set_action(&mut self, action: ViewAction) {
        *self.action = Some(action);
    }
}

pub struct KeybindMatcher<'a> {
    key: KeyEvent,
    action: &'a mut Option<ViewAction>,
    handled: &'a mut bool,
}

impl<'a> KeybindMatcher<'a> {
    pub fn matches<F>(&mut self, binds: &Keybinds, mut cb: F)
    where
        F: FnMut(),
    {
        if *self.handled {
            return;
        }
        if binds.matches(&self.key) {
            *self.handled = true;
            *self.action = Some(ViewAction::None);
            cb();
        }
    }

    pub fn matches_action<F>(&mut self, binds: &Keybinds, mut cb: F)
    where
        F: FnMut(&mut ActionSetter),
    {
        if *self.handled {
            return;
        }
        if binds.matches(&self.key) {
            *self.handled = true;
            *self.action = Some(ViewAction::None);
            let mut setter = ActionSetter {
                action: self.action,
            };
            cb(&mut setter);
        }
    }
}

pub trait KeybindHandler<C> {
    fn handle(&self, ctx: &mut C, matcher: &mut KeybindMatcher);
}

pub struct KeybindRegistry<'a, 'ctx, 'reg> {
    ctx: &'reg mut InputContext<'a, 'ctx>,
    key: KeyEvent,
    action: Option<ViewAction>,
    handled: bool,
}

impl<'a, 'ctx, 'reg> KeybindRegistry<'a, 'ctx, 'reg> {
    pub fn new(ctx: &'reg mut InputContext<'a, 'ctx>, key: KeyEvent) -> Self {
        Self {
            ctx,
            key,
            action: None,
            handled: false,
        }
    }

    pub fn process<H>(mut self, handler: &H) -> Self
    where
        H: KeybindHandler<InputContext<'a, 'ctx>>,
    {
        if !self.handled {
            let mut matcher = KeybindMatcher {
                key: self.key,
                action: &mut self.action,
                handled: &mut self.handled,
            };
            handler.handle(self.ctx, &mut matcher);
        }
        self
    }

    pub fn action(self) -> ViewAction {
        self.action.unwrap_or(ViewAction::None)
    }
}
