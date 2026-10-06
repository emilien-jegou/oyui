//! Locates the enclosing Jujutsu or Git repository.
use std::path::{Path, PathBuf};

/// Which version control system a working directory belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoKind {
    Jujutsu,
    Git,
}

/// A detected repository and the root of its working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub kind: RepoKind,
    pub root: PathBuf,
}

/// Walks up from `start` looking for a Jujutsu or Git repository.
///
/// In a colocated repository (`.jj` and `.git` side by side) Jujutsu wins: the
/// `.jj` directory is the authoritative marker there.
pub fn detect(start: &Path) -> Option<Repo> {
    let mut dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    loop {
        if dir.join(".jj").is_dir() {
            return Some(Repo {
                kind: RepoKind::Jujutsu,
                root: dir.to_path_buf(),
            });
        }
        // `.git` may be a directory (normal repo) or a file (worktree/submodule).
        if dir.join(".git").exists() {
            return Some(Repo {
                kind: RepoKind::Git,
                root: dir.to_path_buf(),
            });
        }
        dir = dir.parent()?;
    }
}

/// Detects the repository enclosing the current working directory.
pub fn detect_cwd() -> Option<Repo> {
    let cwd = std::env::current_dir().ok()?;
    detect(&cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Unique scratch directory per test, mirroring `tree.rs` conventions.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oyui_vcs_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_jujutsu_repo() {
        let dir = scratch("jj");
        fs::create_dir_all(dir.join(".jj")).unwrap();
        let repo = detect(&dir).expect("jj repo");
        assert_eq!(repo.kind, RepoKind::Jujutsu);
        assert_eq!(repo.root, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_git_repo() {
        let dir = scratch("git");
        fs::create_dir_all(dir.join(".git")).unwrap();
        let repo = detect(&dir).expect("git repo");
        assert_eq!(repo.kind, RepoKind::Git);
        assert_eq!(repo.root, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn colocated_repo_prefers_jujutsu() {
        let dir = scratch("colocated");
        fs::create_dir_all(dir.join(".jj")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        assert_eq!(detect(&dir).unwrap().kind, RepoKind::Jujutsu);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_root_from_nested_directory() {
        let dir = scratch("nested");
        fs::create_dir_all(dir.join(".git")).unwrap();
        let nested = dir.join("src/deep");
        fs::create_dir_all(&nested).unwrap();
        let repo = detect(&nested).expect("git repo");
        assert_eq!(repo.kind, RepoKind::Git);
        assert_eq!(repo.root, dir);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn returns_none_without_a_repo() {
        let dir = scratch("none");
        assert!(detect(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
