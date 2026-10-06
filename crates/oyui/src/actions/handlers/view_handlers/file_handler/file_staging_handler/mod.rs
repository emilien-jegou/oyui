use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

pub mod hunk_mutations;
pub mod operations;
pub mod staging_session;
pub mod staging_sync;

use staging_session::StagingSession;

impl ViewFileStagingActionsHandler for AppActionsHandler {
    /// `space` doubles as conflict fold/unfold, which stays available in
    /// merge mode; only the staging fallback is blocked there.
    fn toggle(&self) {
        self.with_staging_session(true, operations::toggle_stage_at_cursor);
    }

    fn toggle_line(&self) {
        self.with_staging_session(false, operations::toggle_single_line_at_cursor);
    }

    fn split(&self) {
        self.with_staging_session(false, operations::split_hunk_at_cursor);
    }

    fn invert(&self) {
        self.with_staging_session(false, operations::invert_staging);
    }

    fn toggle_hunk(&self, val: u32) {
        self.with_staging_session(false, |s| operations::toggle_hunk(s, val as usize));
    }

    fn set_hunk(&self, hunk: u32, staged: bool) {
        self.with_staging_session(false, |s| {
            operations::set_hunk_staged(s, hunk as usize, staged)
        });
    }

    fn split_at(&self, hunk: u32, line: u32) {
        self.with_staging_session(false, |s| {
            operations::split_at(s, hunk as usize, line as usize)
        });
    }

    fn join(&self) {
        self.with_staging_session(false, operations::join_at_cursor);
    }

    fn stage_all(&self) {
        self.with_staging_session(false, |s| operations::set_all(s, true));
    }

    fn unstage_all(&self) {
        self.with_staging_session(false, |s| operations::set_all(s, false));
    }

    fn state(&self) -> String {
        let path = {
            let ui = self.ui.lock();
            ui.file_view.current_path.clone()
        };
        match path.and_then(|p| self.tree.read().get_file_state(&p)) {
            Some(crate::tree::StagingState::Staged) => "staged".into(),
            Some(crate::tree::StagingState::PartiallyStaged) => "partial".into(),
            Some(crate::tree::StagingState::Unstaged) => "unstaged".into(),
            None => "none".into(),
        }
    }
}

impl AppActionsHandler {
    /// Runs `f` with a staging session unless staging is disabled.
    /// `allow_in_merge` keeps conflict fold/unfold (`space`) working in
    /// merge mode; the staging fallback is still rejected in the operation.
    fn with_staging_session<F: FnOnce(&StagingSession)>(&self, allow_in_merge: bool, f: F) {
        if self.reject_read_only() {
            return;
        }
        if !allow_in_merge && self.reject_merge() {
            return;
        }
        // Resolve the session first so a no-op (no open file) does not push a
        // spurious undo point; snapshot only once we are about to mutate.
        let Some(s) = StagingSession::try_new(
            self.tree.clone(),
            self.cache.clone(),
            self.ui.clone(),
            self.operation,
        ) else {
            return;
        };
        self.push_undo_snapshot();
        f(&s);
    }
}
