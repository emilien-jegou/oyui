//! Notification that the user opened a file in the file view.
use std::path::PathBuf;

#[derive(Clone)]
pub struct FileOpened {
    pub path: PathBuf,
}
