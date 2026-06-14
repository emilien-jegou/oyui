use std::path::PathBuf;
use syntect::highlighting::Style;

#[derive(Clone)]
pub struct FileSyntaxUpdate {
    pub node_path: PathBuf,
    pub highlighted: Vec<Vec<(Style, String)>>,
}

impl std::fmt::Debug for FileSyntaxUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSyntaxUpdate")
            .field("path", &self.node_path)
            .finish()
    }
}
