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
    write_temp_then_rename(path, bytes, path)
}

/// Copies `from` onto `to` atomically (the [`std::fs::copy`] equivalent).
///
/// Same staging as [`write_atomic`], but the permission bits applied to the
/// replacement come from `from`, matching `copy`'s semantics: a restored side
/// keeps the mode the snapshot it came from had. The data crosses memory, so
/// this is only for text-sized restores, which is all the app copies.
pub fn copy_atomic(from: &Path, to: &Path) -> io::Result<()> {
    let bytes = fs::read(from)?;
    write_temp_then_rename(to, &bytes, from)
}

/// Stages `bytes` in a sibling temp of `to`, syncs them to disk, applies the
/// permission bits of `perms_from`, and renames over `to`.
fn write_temp_then_rename(to: &Path, bytes: &[u8], perms_from: &Path) -> io::Result<()> {
    let parent = match to.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;

    if let Ok(meta) = fs::metadata(perms_from) {
        tmp.as_file().set_permissions(meta.permissions())?;
    }

    tmp.persist(to).map_err(|err| err.error)?;

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

    #[cfg(unix)]
    #[test]
    fn copy_carries_the_source_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let from = dir.path().join("left.sh");
        fs::write(&from, "restored\n").expect("seed source");
        fs::set_permissions(&from, fs::Permissions::from_mode(0o640)).expect("chmod");

        let to = dir.path().join("dest.sh");
        fs::write(&to, "wrong\n").expect("seed dest");
        fs::set_permissions(&to, fs::Permissions::from_mode(0o600)).expect("chmod");

        copy_atomic(&from, &to).expect("atomic copy");

        assert_eq!(fs::read_to_string(&to).expect("read"), "restored\n");
        // `std::fs::copy` semantics: the source's mode wins over the target's.
        let mode = fs::metadata(&to).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn copy_replaces_existing_content() {
        let dir = tempfile::tempdir().expect("tempdir");
        let from = dir.path().join("left.txt");
        fs::write(&from, "restored\n").expect("seed source");
        let to = dir.path().join("dest.txt");
        fs::write(&to, "wrong\n").expect("seed dest");

        copy_atomic(&from, &to).expect("atomic copy");

        assert_eq!(fs::read_to_string(&to).expect("read"), "restored\n");
        assert!(
            fs::read_dir(dir.path()).expect("dir").count() == 2,
            "no temp litter beside the two real files"
        );
    }
}
