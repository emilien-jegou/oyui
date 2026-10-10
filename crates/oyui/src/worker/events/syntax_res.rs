//! Notification that a file's cached syntax highlighting was recomputed.
use std::path::PathBuf;

/// Syntax highlighting landed in the cache for `path`.
///
/// Highlighting finishes asynchronously and nothing else announces it, so the
/// view needs this to repaint the first time a file's colors appear — without
/// it the app has to poll on a timer just to notice.
#[derive(Clone)]
pub struct SyntaxRes {
    pub path: PathBuf,
}
