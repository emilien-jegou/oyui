//! Git command construction: drives `git` with oyui as the difftool.
use super::tool::{shell_quote, ToolInvocation, ToolProgram};
use crate::cli::{DiffCommandArgs, InterdiffArgs, ResolveArgs, ViewArgs};
use std::path::PathBuf;

/// The `difftool.oyui.cmd` string, run by git through the shell.
fn difftool_cmd(program: &ToolProgram, view: &ViewArgs) -> String {
    let mut parts = vec![shell_quote(&program.exe.to_string_lossy())];
    if let Some(config) = &program.config {
        parts.push("--config".to_string());
        parts.push(shell_quote(&config.to_string_lossy()));
    }
    parts.push("git".to_string());
    parts.push("difftool".to_string());
    parts.extend(view.forwarded());
    parts.push("\"$LOCAL\"".to_string());
    parts.push("\"$REMOTE\"".to_string());
    parts.join(" ")
}

/// Builds `git -c difftool.oyui.cmd=... difftool -y --tool=oyui [revs]`.
fn difftool(
    program: &ToolProgram,
    revs: &[String],
    paths: &[String],
    view: &ViewArgs,
) -> ToolInvocation {
    let mut args = vec![
        "-c".to_string(),
        format!("difftool.oyui.cmd={}", difftool_cmd(program, view)),
        "-c".to_string(),
        "difftool.prompt=false".to_string(),
        "difftool".to_string(),
        "-y".to_string(),
        "--tool=oyui".to_string(),
    ];
    args.extend(revs.iter().cloned());
    if !paths.is_empty() {
        args.push("--".to_string());
        args.extend(paths.iter().cloned());
    }
    ToolInvocation {
        program: PathBuf::from("git"),
        args,
    }
}

/// Opens the working-tree diff (`oyui` standalone on git).
pub fn current_change(program: &ToolProgram) -> ToolInvocation {
    difftool(program, &[], &[], &ViewArgs::default())
}

/// Shows a diff (`oyui diff` on git); revsets are not supported.
pub fn diff(program: &ToolProgram, args: &DiffCommandArgs) -> Result<ToolInvocation, String> {
    let mut revs = Vec::new();
    match (&args.from, &args.to) {
        (Some(from), Some(to)) => {
            revs.push(from.clone());
            revs.push(to.clone());
        }
        (Some(rev), None) | (None, Some(rev)) => revs.push(rev.clone()),
        (None, None) => {
            if let Some(r) = &args.revisions {
                revs.push(r.clone());
            }
        }
    }
    Ok(difftool(program, &revs, &args.paths, &args.view))
}

/// Shows an interdiff between two revisions.
///
/// Git has no native interdiff, so we reproduce jj's definition: rebase the
/// `from` patch onto `to`'s parent with a conflict-free `merge-tree` (one side
/// is the merge base), then diff that tree against `to`.
pub fn interdiff(
    program: &ToolProgram,
    args: &InterdiffArgs,
    root: &std::path::Path,
) -> Result<ToolInvocation, String> {
    let from = args.from.clone().unwrap_or_else(|| "HEAD^".to_string());
    let to = args.to.clone().unwrap_or_else(|| "HEAD".to_string());

    let from_parent = git_stdout(root, &["rev-parse", &format!("{from}^")])
        .map_err(|_| format!("{from} has no parent to rebase from"))?;
    let to_parent = git_stdout(root, &["rev-parse", &format!("{to}^")])
        .map_err(|_| format!("{to} has no parent"))?;

    let rebased = git_stdout(
        root,
        &[
            "merge-tree",
            "--write-tree",
            &format!("--merge-base={from_parent}"),
            &to_parent,
            &from,
        ],
    )
    .map_err(|e| format!("cannot rebase {from} onto {to}'s parent: {e}"))?;
    let rebased_tree = rebased
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    let to_tree = git_stdout(root, &["rev-parse", &format!("{to}^{{tree}}")])?;

    Ok(difftool(
        program,
        &[rebased_tree, to_tree],
        &args.paths,
        &args.view,
    ))
}

/// Runs a git command in `root` and returns its trimmed stdout.
fn git_stdout(root: &std::path::Path, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The `mergetool.oyui.cmd` string, run by git through the shell.
fn mergetool_cmd(program: &ToolProgram) -> String {
    let mut parts = vec![shell_quote(&program.exe.to_string_lossy())];
    if let Some(config) = &program.config {
        parts.push("--config".to_string());
        parts.push(shell_quote(&config.to_string_lossy()));
    }
    parts.push("git".to_string());
    parts.push("mergetool".to_string());
    parts.push("\"$BASE\"".to_string());
    parts.push("\"$LOCAL\"".to_string());
    parts.push("\"$REMOTE\"".to_string());
    parts.push("-o".to_string());
    parts.push("\"$MERGED\"".to_string());
    parts.join(" ")
}

/// Resolves conflicts (`oyui resolve` on git) via `git mergetool`.
pub fn resolve(program: &ToolProgram, args: &ResolveArgs) -> ToolInvocation {
    let mut cmd_args = vec![
        "-c".to_string(),
        format!("mergetool.oyui.cmd={}", mergetool_cmd(program)),
        "-c".to_string(),
        "mergetool.prompt=false".to_string(),
        "mergetool".to_string(),
        "-y".to_string(),
        "--tool=oyui".to_string(),
    ];
    if !args.paths.is_empty() {
        cmd_args.push("--".to_string());
        cmd_args.extend(args.paths.iter().cloned());
    }
    ToolInvocation {
        program: PathBuf::from("git"),
        args: cmd_args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> ToolProgram {
        ToolProgram {
            exe: PathBuf::from("/usr/bin/oyui"),
            config: Some(PathBuf::from("/home/me/oyui.rn")),
        }
    }

    #[test]
    fn current_change_invokes_difftool() {
        let inv = current_change(&program());
        assert_eq!(inv.program, PathBuf::from("git"));
        assert!(inv.args.contains(&"difftool".to_string()));
        assert!(inv.args.contains(&"--tool=oyui".to_string()));
        let cmd = inv
            .args
            .iter()
            .find(|a| a.starts_with("difftool.oyui.cmd="))
            .expect("cmd config");
        assert!(cmd.contains("/usr/bin/oyui"));
        assert!(cmd.contains("git difftool"));
        assert!(cmd.contains("\"$LOCAL\""));
    }

    #[test]
    fn diff_maps_from_and_to_to_two_revs() {
        let args = DiffCommandArgs {
            revisions: None,
            from: Some("main".into()),
            to: Some("@".into()),
            paths: vec!["src".into()],
            ..Default::default()
        };
        let inv = diff(&program(), &args).unwrap();
        let pos = inv.args.iter().position(|a| a == "main").unwrap();
        assert_eq!(inv.args[pos + 1], "@");
        assert!(inv.args.contains(&"--".to_string()));
        assert!(inv.args.contains(&"src".to_string()));
    }

    #[test]
    fn resolve_invokes_mergetool_with_output() {
        let args = ResolveArgs { paths: vec![] };
        let inv = resolve(&program(), &args);
        assert_eq!(inv.program, PathBuf::from("git"));
        assert!(inv.args.contains(&"mergetool".to_string()));
        assert!(inv.args.contains(&"--tool=oyui".to_string()));
        let cmd = inv
            .args
            .iter()
            .find(|a| a.starts_with("mergetool.oyui.cmd="))
            .expect("cmd config");
        assert!(cmd.contains("git mergetool"));
        assert!(cmd.contains("\"$MERGED\""));
    }

    #[test]
    fn interdiff_rebases_from_onto_to_parent() {
        let dir = tempfile::TempDir::new().unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "y@e.com"]);
        run(&["config", "user.name", "E"]);
        std::fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "base"]);
        std::fs::write(dir.path().join("f.txt"), "A\nb\nc\n").unwrap();
        run(&["commit", "-qam", "A"]);
        run(&["checkout", "-q", "-b", "side", "HEAD^"]);
        std::fs::write(dir.path().join("f.txt"), "a\nb\nC\n").unwrap();
        run(&["commit", "-qam", "B"]);

        let args = InterdiffArgs {
            from: Some("main".into()),
            to: Some("side".into()),
            ..Default::default()
        };
        let inv = interdiff(&program(), &args, dir.path()).expect("interdiff");
        let trees: Vec<_> = inv.args.iter().rev().take(2).collect();
        assert!(
            trees
                .iter()
                .all(|t| t.len() == 40 && t.chars().all(|c| c.is_ascii_hexdigit())),
            "expected two tree ids, got {trees:?}"
        );
        assert_ne!(trees[0], trees[1]);
    }

    #[test]
    fn interdiff_without_parent_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success());
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.email", "y@e.com"]);
        run(&["config", "user.name", "E"]);
        std::fs::write(dir.path().join("f.txt"), "a\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-qm", "base"]);

        let args = InterdiffArgs::default();
        assert!(interdiff(&program(), &args, dir.path()).is_err());
    }

    #[test]
    fn difftool_forwards_view_options() {
        let args = DiffCommandArgs {
            view: ViewArgs {
                diff_algorithm: crate::cli::DiffAlgorithm::Myers,
                scrolloff: 5,
                context_lines: 9,
            },
            ..Default::default()
        };
        let inv = diff(&program(), &args).unwrap();
        let cmd = inv
            .args
            .iter()
            .find(|a| a.starts_with("difftool.oyui.cmd="))
            .expect("cmd config");
        assert!(cmd.contains("--diff-algorithm"));
        assert!(cmd.contains("myers"));
        assert!(cmd.contains("--scrolloff"));
        assert!(cmd.contains("--context-lines"));
    }
}
