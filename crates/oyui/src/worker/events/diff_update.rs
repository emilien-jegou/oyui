//! Notification that a file's cached diff was recomputed.
use std::path::PathBuf;

use crate::diff::DiffResult;

/// An event indicating that a file's diff information has been updated.
#[derive(Clone)]
pub struct DiffUpdate {
    pub path: PathBuf,
    pub diff_result: DiffResult,
}
