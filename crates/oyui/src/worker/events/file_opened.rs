use std::path::PathBuf;

#[derive(Clone)]
pub struct FileOpened {
    pub path: PathBuf,
}
impl std::fmt::Debug for FileOpened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileOpened")
            .field("path", &self.path)
            .finish()
    }
}
