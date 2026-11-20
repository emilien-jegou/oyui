//! Filesystem helpers shared by the write paths.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Writes `bytes` to `path` atomically.
///
/// The data goes to a sibling temporary file which is flushed and renamed over
/// the target, so a crash or full disk leaves either the old file or the new
/// one — never a truncated mixture. When the target already exists its
/// permission bits are carried over to the replacement (a merge target can be
/// an executable), and the parent directory is synced so the rename is durable
/// where the platform allows it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;

    if let Ok(meta) = fs::metadata(path) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }

    tmp.persist(path).map_err(|err| err.error)?;

    // Best effort: directories cannot be opened on every platform.
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");
        fs::write(&path, "old\n").expect("seed");

        write_atomic(&path, b"new\n").expect("atomic write");

        assert_eq!(fs::read_to_string(&path).expect("read"), "new\n");
    }

    #[test]
    fn creates_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");

        write_atomic(&path, b"created\n").expect("atomic write");

        assert_eq!(fs::read_to_string(&path).expect("read"), "created\n");
    }

    #[cfg(unix)]
    #[test]
    fn preserves_target_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("script.sh");
        fs::write(&path, "old\n").expect("seed");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");

        write_atomic(&path, b"new\n").expect("atomic write");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }
}
