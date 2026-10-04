use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

pub mod hunk_mutations;
pub mod operations;
pub mod staging_session;
pub mod staging_sync;

use staging_session::StagingSession;

impl ViewFileStagingActionsHandler for AppActionsHandler {
    fn toggle(&self) {
        self.with_staging_session(operations::toggle_stage_at_cursor);
    }

    fn toggle_line(&self) {
        self.with_staging_session(operations::toggle_single_line_at_cursor);
    }

    fn split(&self) {
        self.with_staging_session(operations::split_hunk_at_cursor);
    }

    fn invert(&self) {
        self.with_staging_session(operations::invert_staging);
    }

    fn toggle_hunk(&self, val: u32) {
        self.with_staging_session(|s| operations::toggle_hunk(s, val as usize));
    }

    fn set_hunk(&self, hunk: u32, staged: bool) {
        self.with_staging_session(|s| operations::set_hunk_staged(s, hunk as usize, staged));
    }

    fn split_at(&self, hunk: u32, line: u32) {
        self.with_staging_session(|s| operations::split_at(s, hunk as usize, line as usize));
    }

    fn join(&self) {
        self.with_staging_session(operations::join_at_cursor);
    }

    fn stage_all(&self) {
        self.with_staging_session(|s| operations::set_all(s, true));
    }

    fn unstage_all(&self) {
        self.with_staging_session(|s| operations::set_all(s, false));
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
    fn with_staging_session<F: FnOnce(&StagingSession)>(&self, f: F) {
        // Resolve the session first so a no-op (no open file) does not push a
        // spurious undo point; snapshot only once we are about to mutate.
        let Some(s) =
            StagingSession::try_new(self.tree.clone(), self.cache.clone(), self.ui.clone())
        else {
            return;
        };
        self.push_undo_snapshot();
        f(&s);
    }
}
