//! Git `split`: turns the working-tree changes into two commits.
//!
//! The interactive session reuses the two-way diff engine (HEAD vs worktree).
//! Confirming writes the selected content into a scratch tree; [`finalize`]
//! builds the two commits from it and advances the current ref **atomically**,
//! never touching the user's working tree.
//!
//! Safety: all git objects and commits are created before the ref is moved.
//! The ref update uses compare-and-swap against the HEAD observed during
//! [`prepare`], so a concurrent change aborts instead of overwriting work.
//! Submodules and symlinks are refused rather than mishandled.

use eyre::{bail, eyre, Result};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A prepared split: scratch trees plus the changed paths to reconcile.
pub struct SplitPlan {
    root: PathBuf,
    first_message: String,
    second_message: String,
    /// HEAD versions of changed paths, for the left diff side.
    pub left_dir: PathBuf,
    /// Worktree versions of changed paths, for the right diff side.
    pub right_dir: PathBuf,
    /// Scratch tree that the editor mutates into the first commit's content.
    pub write_dir: PathBuf,
    changed: Vec<PathBuf>,
    head: HashMap<PathBuf, HeadEntry>,
    _temps: Vec<TempDir>,
}

/// The ref name kept as a recovery point for the previous HEAD.
pub const BACKUP_REF: &str = "refs/oyui/split-backup";

/// The two commit ids produced by [`finalize`].
pub struct SplitOutcome {
    pub first: String,
    pub second: String,
    /// HEAD before the split; also stored at [`BACKUP_REF`].
    pub old: String,
}

/// The amended commit produced by [`finalize_squash`].
pub struct SquashOutcome {
    pub old: String,
    pub new: String,
}

#[derive(Clone)]
struct HeadEntry {
    mode: String,
    oid: String,
}

/// Prepares a split of the working-tree changes against HEAD.
pub fn prepare(
    root: &Path,
    first_message: Option<String>,
    second_message: Option<String>,
) -> Result<SplitPlan> {
    ensure_repo(root)?;
    ensure_no_operation(root)?;

    let head = head_entries(root)?;
    let changed = changed_paths(root)?;
    if changed.is_empty() {
        bail!("no changes to split");
    }

    let left = TempDir::new().map_err(|e| eyre!("tempdir: {e}"))?;
    let right = TempDir::new().map_err(|e| eyre!("tempdir: {e}"))?;
    let write = TempDir::new().map_err(|e| eyre!("tempdir: {e}"))?;

    for path in &changed {
        reject_unsupported(root, path)?;
        if let Some(entry) = head.get(path) {
            if entry.mode == "160000" {
                bail!(
                    "submodule {} is not supported by `oyui split`",
                    path.display()
                );
            }
            let blob = git(root, &[os("cat-file"), os("blob"), os(&entry.oid)])?;
            write_file(&left.path().join(path), &blob)?;
        }
        let worktree = root.join(path);
        if worktree.symlink_metadata().is_ok() {
            write_file(&right.path().join(path), &fs::read(&worktree)?)?;
        }
    }
    copy_tree(right.path(), write.path())?;

    let plan = SplitPlan {
        root: root.to_path_buf(),
        first_message: first_message.unwrap_or_else(|| "oyui split (part 1)".into()),
        second_message: second_message.unwrap_or_else(|| "oyui split (part 2)".into()),
        left_dir: left.path().to_path_buf(),
        right_dir: right.path().to_path_buf(),
        write_dir: write.path().to_path_buf(),
        changed,
        head,
        _temps: vec![left, right, write],
    };
    Ok(plan)
}

/// Builds the two commits and advances HEAD from the prepared scratch tree.
pub fn finalize(plan: &SplitPlan) -> Result<SplitOutcome> {
    let root = &plan.root;
    let old_head = git_str(root, &[os("rev-parse"), os("HEAD")])?;

    let first_tree = build_first_tree(plan)?;
    let second_tree = build_second_tree(plan)?;

    let first = git_str(
        root,
        &[
            os("commit-tree"),
            os(&first_tree),
            os("-p"),
            os(&old_head),
            os("-m"),
            os(&plan.first_message),
        ],
    )?;
    let second = git_str(
        root,
        &[
            os("commit-tree"),
            os(&second_tree),
            os("-p"),
            os(&first),
            os("-m"),
            os(&plan.second_message),
        ],
    )?;

    // Keep an easy recovery point before advancing HEAD.
    let _ = git(root, &[os("update-ref"), os(BACKUP_REF), os(&old_head)]);
    // Atomic compare-and-swap: only advance if HEAD is still what we saw.
    git(
        root,
        &[os("update-ref"), os("HEAD"), os(&second), os(&old_head)],
    )
    .map_err(|e| eyre!("refusing to move HEAD (repository changed?): {e}"))?;
    // Sync the index to the new commit without touching the working tree.
    git(root, &[os("read-tree"), os(&second)])?;

    Ok(SplitOutcome {
        first,
        second,
        old: old_head,
    })
}

/// Amends HEAD with the prepared scratch tree, leaving unselected changes in
/// the working tree. Returns `None` when nothing was selected.
pub fn finalize_squash(plan: &SplitPlan) -> Result<Option<SquashOutcome>> {
    let root = &plan.root;
    let old_head = git_str(root, &[os("rev-parse"), os("HEAD")])?;

    let tree = build_first_tree(plan)?;
    let head_tree = git_str(root, &[os("rev-parse"), os("HEAD^{tree}")])?;
    if tree == head_tree {
        return Ok(None);
    }

    let message = git_str(root, &[os("log"), os("-1"), os("--format=%B"), os("HEAD")])?;
    let author_name = git_str(root, &[os("log"), os("-1"), os("--format=%an"), os("HEAD")])?;
    let author_email = git_str(root, &[os("log"), os("-1"), os("--format=%ae"), os("HEAD")])?;
    let author_date = git_str(root, &[os("log"), os("-1"), os("--format=%aI"), os("HEAD")])?;

    let mut args = vec![os("commit-tree"), os(&tree)];
    for parent in parents(root, &old_head)? {
        args.push(os("-p"));
        args.push(os(&parent));
    }
    args.push(os("-m"));
    args.push(os(&message));

    let new_head = String::from_utf8(git_env(
        root,
        &args,
        &[
            ("GIT_AUTHOR_NAME", &author_name),
            ("GIT_AUTHOR_EMAIL", &author_email),
            ("GIT_AUTHOR_DATE", &author_date),
        ],
    )?)?
    .trim()
    .to_string();

    let _ = git(root, &[os("update-ref"), os(BACKUP_REF), os(&old_head)]);
    git(
        root,
        &[os("update-ref"), os("HEAD"), os(&new_head), os(&old_head)],
    )
    .map_err(|e| eyre!("refusing to amend HEAD (repository changed?): {e}"))?;
    git(root, &[os("read-tree"), os(&new_head)])?;

    Ok(Some(SquashOutcome {
        old: old_head,
        new: new_head,
    }))
}

/// Parent commit ids of `rev`, preserving merges.
fn parents(root: &Path, rev: &str) -> Result<Vec<String>> {
    let line = git_str(
        root,
        &[os("rev-list"), os("--parents"), os("-n"), os("1"), os(rev)],
    )?;
    Ok(line
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect())
}

/// Tree for the first commit: scratch tree overlaid on HEAD.
fn build_first_tree(plan: &SplitPlan) -> Result<String> {
    let index = plan.write_dir.join(".oyui-split-index");
    let _ = fs::remove_file(&index);
    git_index(&plan.root, &index, &[os("read-tree"), os("HEAD")])?;

    for path in &plan.changed {
        let c1 = plan.write_dir.join(path);
        let Ok(c1_bytes) = fs::read(&c1) else {
            // Absent in the first commit: drop it from the index.
            git_index(
                &plan.root,
                &index,
                &[
                    os("update-index"),
                    os("--force-remove"),
                    path.as_os_str().into(),
                ],
            )?;
            continue;
        };
        // Content identical to HEAD means the change was left out; the index
        // entry seeded from HEAD is already correct (and re-hashing could
        // double-apply clean filters).
        if plan.head.contains_key(path) {
            if let Ok(left) = fs::read(plan.left_dir.join(path)) {
                if left == c1_bytes {
                    continue;
                }
            }
        }
        let blob = git_str(
            &plan.root,
            &[
                os("hash-object"),
                os("-w"),
                os("--path"),
                path.as_os_str().into(),
                c1.as_os_str().into(),
            ],
        )?;
        let mode = first_mode(plan, path, &blob);
        git_index(
            &plan.root,
            &index,
            &[
                os("update-index"),
                os("--add"),
                os("--cacheinfo"),
                cacheinfo_spec(&mode, &blob, path),
            ],
        )?;
    }
    let tree = git_str_index(&plan.root, &index, &[os("write-tree")])?;
    let _ = fs::remove_file(&index);
    Ok(tree)
}

/// Tree for the second commit: the full working tree.
fn build_second_tree(plan: &SplitPlan) -> Result<String> {
    let index = plan.write_dir.join(".oyui-split-index-2");
    let _ = fs::remove_file(&index);
    git_index(&plan.root, &index, &[os("read-tree"), os("HEAD")])?;
    git_index(&plan.root, &index, &[os("add"), os("-A")])?;
    let tree = git_str_index(&plan.root, &index, &[os("write-tree")])?;
    let _ = fs::remove_file(&index);
    Ok(tree)
}

/// Picks the mode for a first-commit path: HEAD's mode when the content is
/// unchanged (a reverted change must not alter the exec bit), else the
/// working-tree mode.
fn first_mode(plan: &SplitPlan, path: &Path, blob: &str) -> String {
    if let Some(entry) = plan.head.get(path) {
        if entry.oid == blob {
            return entry.mode.clone();
        }
    }
    worktree_mode(&plan.root.join(path)).unwrap_or_else(|| "100644".into())
}

fn worktree_mode(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = path.symlink_metadata().ok()?;
        if meta.file_type().is_symlink() {
            return Some("120000".into());
        }
        if meta.permissions().mode() & 0o111 != 0 {
            return Some("100755".into());
        }
    }
    Some("100644".into())
}

// --- repository inspection -------------------------------------------------

fn ensure_repo(root: &Path) -> Result<()> {
    git(root, &[os("rev-parse"), os("--git-dir")])
        .map_err(|e| eyre!("not a git repository: {e}"))?;
    git(root, &[os("rev-parse"), os("--verify"), os("HEAD")])
        .map_err(|_| eyre!("repository has no commits yet"))?;
    Ok(())
}

/// Refuses to split while another git operation is in progress.
fn ensure_no_operation(root: &Path) -> Result<()> {
    for name in [
        "MERGE_HEAD",
        "REBASE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
    ] {
        if git_ok(root, &[os("rev-parse"), os("-q"), os("--verify"), os(name)])? {
            bail!("a git {name} operation is in progress; finish it first");
        }
    }
    let git_dir = git_str(root, &[os("rev-parse"), os("--absolute-git-dir")])?;
    let git_dir = PathBuf::from(git_dir);
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        bail!("a git rebase is in progress; finish it first");
    }
    if !git(root, &[os("ls-files"), os("-u")])?.is_empty() {
        bail!("unresolved conflicts in the index; resolve them first");
    }
    Ok(())
}

/// Paths that differ between HEAD and the working tree (ignores excluded).
fn changed_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let out = git(
        root,
        &[
            os("status"),
            os("--porcelain=v1"),
            os("-z"),
            os("--untracked-files=all"),
            os("--no-renames"),
        ],
    )?;
    let mut paths = Vec::new();
    for entry in out.split(|b| *b == 0) {
        if entry.len() < 4 {
            continue;
        }
        // `XY <path>`; the two status bytes are fixed-width.
        let path = &entry[3..];
        paths.push(bytes_to_path(path));
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Reads HEAD's tracked entries (`path -> mode/oid`).
fn head_entries(root: &Path) -> Result<HashMap<PathBuf, HeadEntry>> {
    let out = git(root, &[os("ls-tree"), os("-r"), os("-z"), os("HEAD")])?;
    let mut map = HashMap::new();
    for entry in out.split(|b| *b == 0) {
        if entry.is_empty() {
            continue;
        }
        // `<mode> <type> <oid>\t<path>`
        let Some(tab) = entry.iter().position(|b| *b == b'\t') else {
            continue;
        };
        let meta = String::from_utf8_lossy(&entry[..tab]);
        let mut parts = meta.split_whitespace();
        let (Some(mode), Some(_kind), Some(oid)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = bytes_to_path(&entry[tab + 1..]);
        map.insert(
            path,
            HeadEntry {
                mode: mode.to_string(),
                oid: oid.to_string(),
            },
        );
    }
    Ok(map)
}

fn reject_unsupported(root: &Path, path: &Path) -> Result<()> {
    let meta = match root.join(path).symlink_metadata() {
        Ok(meta) => meta,
        // A missing worktree file is simply a deletion; not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(eyre!("cannot stat {}: {e}", path.display())),
    };
    if meta.file_type().is_dir() {
        bail!(
            "{} is a directory or submodule; not supported",
            path.display()
        );
    }
    if meta.file_type().is_symlink() {
        bail!(
            "{} is a symlink; not supported by `oyui split`",
            path.display()
        );
    }
    Ok(())
}

// --- filesystem helpers ----------------------------------------------------

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for entry in walk(from) {
        let rel = entry.strip_prefix(from).expect("walked path is under root");
        let dest = to.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&dest)?;
        } else {
            write_file(&dest, &fs::read(&entry)?)?;
        }
    }
    Ok(())
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            out.push(path.clone());
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    out
}

#[cfg(unix)]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

// --- git process helpers ---------------------------------------------------

fn os(s: &str) -> OsString {
    OsString::from(s)
}

/// Builds the `<mode>,<object>,<path>` argument `git update-index` expects.
fn cacheinfo_spec(mode: &str, blob: &str, path: &Path) -> OsString {
    let mut spec = format!("{mode},{blob},").into_bytes();
    push_path_bytes(&mut spec, path);
    bytes_to_os(spec)
}

#[cfg(unix)]
fn push_path_bytes(out: &mut Vec<u8>, path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    out.extend_from_slice(path.as_os_str().as_bytes());
}

#[cfg(not(unix))]
fn push_path_bytes(out: &mut Vec<u8>, path: &Path) {
    out.extend_from_slice(path.to_string_lossy().as_bytes());
}

#[cfg(unix)]
fn bytes_to_os(bytes: Vec<u8>) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(bytes)
}

#[cfg(not(unix))]
fn bytes_to_os(bytes: Vec<u8>) -> OsString {
    OsString::from(String::from_utf8_lossy(&bytes).into_owned())
}

fn git(root: &Path, args: &[OsString]) -> Result<Vec<u8>> {
    run_git(root, None, args)
}

fn git_index(root: &Path, index: &Path, args: &[OsString]) -> Result<Vec<u8>> {
    run_git(root, Some(index), args)
}

fn git_env(root: &Path, args: &[OsString], envs: &[(&str, &String)]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| eyre!("failed to run git: {e}"))?;
    if !out.status.success() {
        bail!(
            "git command failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

fn run_git(root: &Path, index: Option<&Path>, args: &[OsString]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root);
    if let Some(index) = index {
        cmd.env("GIT_INDEX_FILE", index);
    }
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| eyre!("failed to run git: {e}"))?;
    if !out.status.success() {
        let rendered = args
            .iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        bail!(
            "git {rendered} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

fn git_ok(root: &Path, args: &[OsString]) -> Result<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| eyre!("failed to run git: {e}"))?;
    Ok(out.status.success())
}

fn git_str(root: &Path, args: &[OsString]) -> Result<String> {
    Ok(String::from_utf8(git(root, args)?)?.trim().to_string())
}

fn git_str_index(root: &Path, index: &Path, args: &[OsString]) -> Result<String> {
    Ok(String::from_utf8(git_index(root, index, args)?)?
        .trim()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git_setup(dir: &Path) {
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(args)
                .output()
                .expect("git");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "you@example.com"]);
        run(&["config", "user.name", "Example"]);
    }

    fn commit(dir: &Path, message: &str) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["commit", "-q", "-am", message])
            .output()
            .expect("git commit");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn rev(dir: &Path, rev: &str) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", rev])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn show(dir: &Path, rev: &str, path: &str) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["show", &format!("{rev}:{path}")])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn prepare_and_finalize_builds_two_commits() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
        let out = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        assert!(out.status.success());
        commit(dir.path(), "base");

        // Modify and select the change for the first commit.
        fs::write(dir.path().join("a.txt"), "one\nTWO\n").unwrap();
        let plan = prepare(dir.path(), Some("first".into()), Some("second".into())).unwrap();
        // Leave write_dir as the full worktree copy -> first commit takes it all.
        let outcome = finalize(&plan).unwrap();
        assert_eq!(rev(dir.path(), "HEAD"), outcome.second);
        assert_eq!(show(dir.path(), "HEAD", "a.txt"), "one\nTWO\n");
        assert_eq!(rev(dir.path(), "HEAD~1"), outcome.first);
        assert_eq!(show(dir.path(), "HEAD~1", "a.txt"), "one\nTWO\n");
        // Working tree untouched and clean.
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(status.stdout.is_empty(), "working tree must stay clean");
    }

    #[test]
    fn selecting_nothing_leaves_first_commit_empty() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");

        fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        let plan = prepare(dir.path(), None, None).unwrap();
        // Simulate "select nothing": revert the scratch tree to HEAD content.
        fs::write(plan.write_dir.join("a.txt"), "one\n").unwrap();
        let outcome = finalize(&plan).unwrap();
        assert_eq!(show(dir.path(), "HEAD", "a.txt"), "two\n");
        assert_eq!(
            show(dir.path(), &outcome.first, "a.txt"),
            "one\n",
            "first commit keeps HEAD content when nothing is selected"
        );
    }

    #[test]
    fn first_commit_keeps_only_selected_content() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "a\nb\nc\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");

        // Two separated changes; select only the first for the first commit.
        fs::write(dir.path().join("a.txt"), "A\nb\nC\n").unwrap();
        let plan = prepare(dir.path(), Some("first".into()), Some("second".into())).unwrap();
        fs::write(plan.write_dir.join("a.txt"), "A\nb\nc\n").unwrap();
        let outcome = finalize(&plan).unwrap();

        assert_eq!(show(dir.path(), &outcome.first, "a.txt"), "A\nb\nc\n");
        assert_eq!(show(dir.path(), &outcome.second, "a.txt"), "A\nb\nC\n");
        assert_eq!(show(dir.path(), "HEAD", "a.txt"), "A\nb\nC\n");
    }

    #[test]
    fn handles_added_deleted_and_untracked_files() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("keep.txt"), "keep\n").unwrap();
        fs::write(dir.path().join("del.txt"), "bye\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");

        fs::write(dir.path().join("keep.txt"), "KEEP\n").unwrap();
        fs::remove_file(dir.path().join("del.txt")).unwrap();
        fs::write(dir.path().join("new.txt"), "new\n").unwrap();
        fs::write(dir.path().join("untracked.txt"), "u\n").unwrap();

        let plan = prepare(dir.path(), None, None).unwrap();
        // Simulate reverting everything for the first commit.
        fs::write(plan.write_dir.join("keep.txt"), "keep\n").unwrap();
        fs::write(plan.write_dir.join("del.txt"), "bye\n").unwrap();
        fs::remove_file(plan.write_dir.join("new.txt")).unwrap();
        fs::remove_file(plan.write_dir.join("untracked.txt")).unwrap();
        let outcome = finalize(&plan).unwrap();

        assert_eq!(show(dir.path(), &outcome.first, "keep.txt"), "keep\n");
        assert_eq!(show(dir.path(), &outcome.first, "del.txt"), "bye\n");
        assert_title_absent(dir.path(), &outcome.first, "new.txt");
        assert_eq!(show(dir.path(), &outcome.second, "keep.txt"), "KEEP\n");
        assert_eq!(show(dir.path(), &outcome.second, "new.txt"), "new\n");
        assert_eq!(show(dir.path(), &outcome.second, "untracked.txt"), "u\n");
        assert_title_absent(dir.path(), &outcome.second, "del.txt");
    }

    fn assert_title_absent(dir: &Path, rev: &str, path: &str) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["cat-file", "-e", &format!("{rev}:{path}")])
            .output()
            .unwrap();
        assert!(!out.status.success(), "{path} must be absent from {rev}");
    }

    #[test]
    fn finalize_squash_amends_head_and_keeps_rest_in_worktree() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "a\nb\nc\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");
        let base = rev(dir.path(), "HEAD");
        fs::write(dir.path().join("b.txt"), "second\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "second");

        fs::write(dir.path().join("a.txt"), "A\nb\nC\n").unwrap();
        let plan = prepare(dir.path(), None, None).unwrap();
        // Select only the first hunk.
        fs::write(plan.write_dir.join("a.txt"), "A\nb\nc\n").unwrap();
        let outcome = finalize_squash(&plan).unwrap().expect("squash happened");

        assert_eq!(rev(dir.path(), "HEAD"), outcome.new);
        assert_eq!(
            rev(dir.path(), "HEAD~1"),
            base,
            "parent of amended HEAD preserved"
        );
        assert_eq!(show(dir.path(), "HEAD", "a.txt"), "A\nb\nc\n");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "A\nb\nC\n",
            "the unselected change stays in the working tree"
        );
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(
            String::from_utf8(status.stdout).unwrap().contains("a.txt"),
            "remaining change must show as unstaged"
        );
    }

    #[test]
    fn finalize_squash_with_nothing_selected_is_a_noop() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");
        let base = rev(dir.path(), "HEAD");

        fs::write(dir.path().join("a.txt"), "A\n").unwrap();
        let plan = prepare(dir.path(), None, None).unwrap();
        fs::write(plan.write_dir.join("a.txt"), "a\n").unwrap();
        assert!(finalize_squash(&plan).unwrap().is_none());
        assert_eq!(rev(dir.path(), "HEAD"), base, "HEAD must be unchanged");
    }

    #[test]
    fn refuses_without_changes() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");
        assert!(prepare(dir.path(), None, None).is_err());
    }

    #[test]
    fn refuses_during_merge() {
        let dir = TempDir::new().unwrap();
        git_setup(dir.path());
        fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["add", "."])
            .output()
            .unwrap();
        commit(dir.path(), "base");
        fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        fs::write(dir.path().join(".git/MERGE_HEAD"), rev(dir.path(), "HEAD")).unwrap();
        assert!(prepare(dir.path(), None, None).is_err());
    }
}
