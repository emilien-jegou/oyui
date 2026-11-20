//! Notification that a file's cached diff was recomputed.
use std::path::PathBuf;

/// An event indicating that a file's diff information has been updated.
///
/// Carries only the path: the computed [`crate::diff::DiffResult`] is written
/// to `DiffCache` before this event fires, so listeners read the payload from
/// the cache instead of the bus. Shipping the full diff here deep-cloned every
/// `Hunk` and `DiffLine` once per subscriber for data all of them discarded.
#[derive(Clone)]
pub struct DiffUpdate {
    pub path: PathBuf,
}
