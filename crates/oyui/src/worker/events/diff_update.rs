use std::path::PathBuf;

use crate::diff::DiffResult;

/// An event indicating that a file's diff information has been updated.
#[derive(Clone)]
pub struct DiffUpdate {
    pub path: PathBuf,
    pub diff_result: DiffResult,
}

impl std::fmt::Debug for DiffUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalculateFileTreeReq")
            .field("path", &self.path)
            .finish()
    }
}
