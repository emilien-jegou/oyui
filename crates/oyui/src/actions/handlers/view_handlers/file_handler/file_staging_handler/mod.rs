use crate::actions::handlers::AppActionsHandler;
use crate::actions::*;

pub mod hunk_mutations;
pub mod operations;
pub mod staging_session;
pub mod staging_sync;
pub mod utils;

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
}

impl AppActionsHandler {
    fn with_staging_session<F: FnOnce(&StagingSession)>(&self, f: F) {
        if let Some(s) = StagingSession::try_new(
            self.tree.clone(),
            self.cache.clone(),
            self.view.file_view.clone(),
        ) {
            f(&s);
        }
    }
}
