use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagingState {
    Unstaged,
    PartiallyStaged,
    Staged,
}

impl StagingState {
    pub fn toggle(self) -> Self {
        match self {
            StagingState::Unstaged | StagingState::PartiallyStaged => StagingState::Staged,
            StagingState::Staged => StagingState::Unstaged,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeNodeFile {
    pub name: String,
    pub path: PathBuf,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
    pub state: StagingState,
}

#[derive(Debug, Clone)]
pub struct TreeNodeDirectory {
    pub name: String,
    pub path: PathBuf,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone)]
pub enum TreeNode {
    File(TreeNodeFile),
    Directory(TreeNodeDirectory),
}

#[derive(Debug, Default)]
pub struct FileTree {
    pub nodes: Vec<TreeNode>,
}

impl FileTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.iter().all(|node| match node {
            TreeNode::File(_) => false,
            TreeNode::Directory(d) => d.children.is_empty(),
        })
    }

    #[tracing::instrument(skip_all)]
    pub fn build_from_dir_diff(
        left_dir: &Path,
        right_dir: &Path,
    ) -> (Self, Vec<(PathBuf, PathBuf, PathBuf)>) {
        let mut tree = Self::new();
        let mut files_to_stat = Vec::new();
        
        let mut added_count = 0;
        let mut modified_count = 0;
        let mut deleted_count = 0;

        // 1. Walk right_dir (Modified and Added files)
        tracing::debug!("Walking right directory to discover modified/added files");
        for entry in WalkDir::new(right_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let right_path = entry.path().to_path_buf();
                if let Ok(rel_path) = right_path.strip_prefix(right_dir) {
                    let left_path = left_dir.join(rel_path);
                    let rel_path_buf = rel_path.to_path_buf();

                    if left_path.exists() {
                        if files_are_identical(&left_path, &right_path) {
                            continue;
                        }
                        modified_count += 1;
                        tracing::trace!(path = %rel_path_buf.display(), "Discovered modified file");
                        tree.insert_file(
                            rel_path_buf.clone(),
                            Some(left_path.clone()),
                            Some(right_path.clone()),
                        );
                    } else {
                        // Added file: Pass None for left_path
                        added_count += 1;
                        tracing::trace!(path = %rel_path_buf.display(), "Discovered added file");
                        tree.insert_file(
                            rel_path_buf.clone(),
                            None,
                            Some(right_path.clone()),
                        );
                    }
                    
                    files_to_stat.push((rel_path_buf, left_path, right_path));
                }
            }
        }

        // 2. Walk left_dir (Deleted files missing in right_dir)
        tracing::debug!("Walking left directory to discover deleted files");
        for entry in WalkDir::new(left_dir).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let left_path = entry.path().to_path_buf();
                if let Ok(rel_path) = left_path.strip_prefix(left_dir) {
                    let right_path = right_dir.join(rel_path);
                    if !right_path.exists() {
                        let rel_path_buf = rel_path.to_path_buf();
                        // Deleted file: Pass None for right_path
                        deleted_count += 1;
                        tracing::trace!(path = %rel_path_buf.display(), "Discovered deleted file");
                        tree.insert_file(
                            rel_path_buf.clone(),
                            Some(left_path.clone()),
                            None,
                        );
                        files_to_stat.push((rel_path_buf, left_path, right_path));
                    }
                }
            }
        }

        tracing::info!(
            added = added_count,
            modified = modified_count,
            deleted = deleted_count,
            "Finished building file tree"
        );

        (tree, files_to_stat)
    }

    pub fn insert_file(
        &mut self,
        rel_path: PathBuf,
        left_path: Option<PathBuf>,
        right_path: Option<PathBuf>,
    ) {
        let components: Vec<_> = rel_path
            .iter()
            .map(|c| c.to_string_lossy().into_owned())
            .collect();
        let mut current_nodes = &mut self.nodes;
        let mut current_path = PathBuf::new();
        let len = components.len();

        for (i, name) in components.into_iter().enumerate() {
            current_path.push(&name);
            let is_last = i == len - 1;

            if is_last {
                current_nodes.push(TreeNode::File(TreeNodeFile {
                    name,
                    path: current_path.clone(),
                    left_path: left_path.clone(),
                    right_path: right_path.clone(),
                    state: StagingState::Unstaged,
                }))
            } else {
                let pos = current_nodes.iter().position(|n| match n {
                    TreeNode::Directory(d) => d.name == name,
                    _ => false,
                });

                let idx = if let Some(p) = pos {
                    p
                } else {
                    current_nodes.push(TreeNode::Directory(TreeNodeDirectory {
                        name: name.clone(),
                        path: current_path.clone(),
                        children: Vec::new(),
                    }));
                    current_nodes.len() - 1
                };

                match &mut current_nodes[idx] {
                    TreeNode::Directory(d) => current_nodes = &mut d.children,
                    _ => unreachable!(),
                }
            }
        }
    }
}

impl TreeNode {
    pub fn path(&self) -> &PathBuf {
        match self {
            TreeNode::File(f) => &f.path,
            TreeNode::Directory(d) => &d.path,
        }
    }

    pub fn invert_state_recursive(&mut self) {
        match self {
            TreeNode::File(f) => {
                f.state = match f.state {
                    StagingState::Staged => StagingState::Unstaged,
                    StagingState::Unstaged => StagingState::Staged,
                    StagingState::PartiallyStaged => StagingState::PartiallyStaged,
                };
            }
            TreeNode::Directory(d) => {
                for child in &mut d.children {
                    child.invert_state_recursive();
                }
            }
        }
    }

    pub fn set_state_recursive(&mut self, new_state: StagingState) {
        match self {
            TreeNode::File(file) => file.state = new_state,
            TreeNode::Directory(dir) => {
                for child in &mut dir.children {
                    child.set_state_recursive(new_state);
                }
            }
        }
    }

    pub fn compute_staging_state(&self) -> StagingState {
        match self {
            TreeNode::File(f) => f.state,
            TreeNode::Directory(dir) => {
                if dir.children.is_empty() {
                    return StagingState::Unstaged;
                }
                let mut has_staged = false;
                let mut has_unstaged = false;
                let mut has_partial = false;

                for child in &dir.children {
                    match child.compute_staging_state() {
                        StagingState::Staged => has_staged = true,
                        StagingState::Unstaged => has_unstaged = true,
                        StagingState::PartiallyStaged => has_partial = true,
                    }
                }

                if has_partial || (has_staged && has_unstaged) {
                    StagingState::PartiallyStaged
                } else if has_staged {
                    StagingState::Staged
                } else {
                    StagingState::Unstaged
                }
            }
        }
    }
}

#[tracing::instrument(level = "trace", skip_all)]
fn files_are_identical(path_a: &Path, path_b: &Path) -> bool {
    let meta_a = match fs::metadata(path_a) {
        Ok(m) => m,
        Err(_) => return false,
    };
    let meta_b = match fs::metadata(path_b) {
        Ok(m) => m,
        Err(_) => return false,
    };

    if meta_a.len() != meta_b.len() {
        return false;
    }

    let f1 = match fs::File::open(path_a) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let f2 = match fs::File::open(path_b) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let mut b1 = BufReader::new(f1).bytes();
    let mut b2 = BufReader::new(f2).bytes();

    loop {
        match (b1.next(), b2.next()) {
            (Some(Ok(v1)), Some(Ok(v2))) => {
                if v1 != v2 {
                    return false;
                }
            }
            (None, None) => return true,
            _ => return false,
        }
    }
}
