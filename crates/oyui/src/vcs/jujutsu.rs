//! Jujutsu command construction: drives `jj` with oyui as the external tool.
use super::tool::{toml_string, ToolInvocation, ToolProgram};
use crate::cli::{DiffCommandArgs, InterdiffArgs, ResolveArgs, SplitArgs, SquashArgs};
use std::path::PathBuf;

const TOOL: &str = "oyui";

fn program_config(program: &ToolProgram) -> String {
    format!(
        "merge-tools.{TOOL}.program={}",
        toml_string(&program.exe.to_string_lossy())
    )
}

fn args_config(program: &ToolProgram, key: &str, tool: &[&str], extra: &[String]) -> String {
    let mut items = program.prefix_args();
    items.extend(tool.iter().map(|s| (*s).to_string()));
    items.extend(extra.iter().cloned());
    let array = items
        .iter()
        .map(|s| toml_string(s))
        .collect::<Vec<_>>()
        .join(", ");
    format!("merge-tools.{TOOL}.{key}=[{array}]")
}

/// Builds `jj --config NAME=VALUE ... <subcommand> --tool oyui`.
///
/// `extra` is appended to the tool arguments so oyui view options survive the
/// round-trip through jj.
fn base(
    program: &ToolProgram,
    key: &str,
    tool: &[&str],
    subcommand: &str,
    extra: &[String],
) -> ToolInvocation {
    let args = vec![
        "--no-pager".to_string(),
        "--config".to_string(),
        program_config(program),
        "--config".to_string(),
        args_config(program, key, tool, extra),
        subcommand.to_string(),
        "--tool".to_string(),
        TOOL.to_string(),
    ];
    ToolInvocation {
        program: PathBuf::from("jj"),
        args,
    }
}

/// Edits the working-copy change (`oyui` standalone).
pub fn current_change(program: &ToolProgram) -> ToolInvocation {
    base(
        program,
        "edit-args",
        &["jj", "edittool", "$left", "$right"],
        "diffedit",
        &[],
    )
}

fn push_revs(
    args: &mut Vec<String>,
    revisions: &Option<String>,
    from: &Option<String>,
    to: &Option<String>,
) {
    if let Some(r) = revisions {
        args.push("-r".to_string());
        args.push(r.clone());
    }
    if let Some(f) = from {
        args.push("--from".to_string());
        args.push(f.clone());
    }
    if let Some(t) = to {
        args.push("--to".to_string());
        args.push(t.clone());
    }
}

/// Shows a diff (`oyui diff`); revision arguments mirror `jj diff`.
pub fn diff(program: &ToolProgram, args: &DiffCommandArgs) -> ToolInvocation {
    let mut inv = base(
        program,
        "diff-args",
        &["jj", "difftool", "$left", "$right"],
        "diff",
        &args.view.forwarded(),
    );
    push_revs(&mut inv.args, &args.revisions, &args.from, &args.to);
    inv.args.extend(args.paths.iter().cloned());
    inv
}

/// Splits the current change (`oyui split`); delegates to `jj split` with oyui
/// as the interactive diff editor.
pub fn split(program: &ToolProgram, args: &SplitArgs) -> ToolInvocation {
    let mut inv = base(
        program,
        "edit-args",
        &["jj", "edittool", "$left", "$right"],
        "split",
        &args.view.forwarded(),
    );
    if let Some(revision) = &args.revision {
        inv.args.push("-r".to_string());
        inv.args.push(revision.clone());
    }
    if let Some(message) = &args.message {
        inv.args.push("-m".to_string());
        inv.args.push(message.clone());
    }
    inv.args.extend(args.paths.iter().cloned());
    inv
}

/// Resolves conflicts (`oyui resolve`); delegates to `jj resolve` with oyui as
/// the merge tool.
pub fn resolve(program: &ToolProgram, args: &ResolveArgs) -> ToolInvocation {
    let mut inv = base(
        program,
        "merge-args",
        &["jj", "mergetool", "$base", "$left", "$right"],
        "resolve",
        &[],
    );
    inv.args.extend(args.paths.iter().cloned());
    inv
}

/// Squashes changes into the parent (`oyui squash`); delegates to `jj squash`
/// with oyui as the interactive diff editor.
pub fn squash(program: &ToolProgram, args: &SquashArgs) -> ToolInvocation {
    // Squash folds everything into the parent by default; the editor starts
    // fully staged so confirming with no interaction is meaningful (jj errors
    // with "No changes selected" otherwise).
    let mut extra = args.view.forwarded();
    extra.push("--start-staged".to_string());
    let mut inv = base(
        program,
        "edit-args",
        &["jj", "edittool", "$left", "$right"],
        "squash",
        &extra,
    );
    if let Some(from) = &args.from {
        inv.args.push("--from".to_string());
        inv.args.push(from.clone());
    }
    if let Some(into) = &args.into {
        inv.args.push("--into".to_string());
        inv.args.push(into.clone());
    }
    if args.keep_emptied {
        inv.args.push("--keep-emptied".to_string());
    }
    inv.args.extend(args.paths.iter().cloned());
    inv
}

/// Shows an interdiff (`oyui interdiff`); mirrors `jj interdiff`.
pub fn interdiff(program: &ToolProgram, args: &InterdiffArgs) -> ToolInvocation {
    let mut inv = base(
        program,
        "diff-args",
        &["jj", "difftool", "$left", "$right"],
        "interdiff",
        &args.view.forwarded(),
    );
    // `jj interdiff` requires at least one revision; default to comparing the
    // current change with its parent.
    if args.from.is_none() && args.to.is_none() {
        push_revs(
            &mut inv.args,
            &None,
            &Some("@-".to_string()),
            &Some("@".to_string()),
        );
    } else {
        push_revs(&mut inv.args, &None, &args.from, &args.to);
    }
    inv.args.extend(args.paths.iter().cloned());
    inv
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

    fn contains_pair(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn current_change_uses_edit_args_and_diffedit() {
        let inv = current_change(&program());
        assert_eq!(inv.program, PathBuf::from("jj"));
        assert_eq!(inv.args[5], "diffedit");
        assert!(inv.args.iter().any(|a| a.contains("edit-args")));
        assert!(inv.args.iter().any(|a| a.contains("\"$left\"")));
        assert!(contains_pair(&inv.args, "--tool", "oyui"));
        assert!(inv.args.iter().any(|a| a.contains("/usr/bin/oyui")));
        assert!(inv.args.iter().any(|a| a.contains("/home/me/oyui.rn")));
    }

    #[test]
    fn diff_forwards_revisions_and_paths() {
        let args = DiffCommandArgs {
            revisions: Some("@-".into()),
            from: None,
            to: None,
            paths: vec!["src".into()],
            ..Default::default()
        };
        let inv = diff(&program(), &args);
        assert_eq!(inv.args[5], "diff");
        assert!(inv.args.iter().any(|a| a.contains("diff-args")));
        assert!(contains_pair(&inv.args, "-r", "@-"));
        assert!(inv.args.contains(&"src".to_string()));
    }

    #[test]
    fn split_uses_edit_args_and_forwards_message() {
        let args = SplitArgs {
            message: Some("part one".into()),
            message_second: None,
            revision: Some("@".into()),
            paths: vec![],
            view: crate::cli::ViewArgs {
                diff_algorithm: crate::cli::DiffAlgorithm::Histogram,
                scrolloff: 2,
                context_lines: 4,
            },
        };
        let inv = split(&program(), &args);
        assert_eq!(inv.args[5], "split");
        assert!(inv.args.iter().any(|a| a.contains("edit-args")));
        assert!(contains_pair(&inv.args, "-m", "part one"));
        assert!(contains_pair(&inv.args, "-r", "@"));
        assert!(contains_pair(&inv.args, "--tool", "oyui"));
    }

    #[test]
    fn view_options_are_forwarded_to_the_tool() {
        let args = DiffCommandArgs {
            view: crate::cli::ViewArgs {
                diff_algorithm: crate::cli::DiffAlgorithm::Myers,
                scrolloff: 5,
                context_lines: 9,
            },
            ..Default::default()
        };
        let inv = diff(&program(), &args);
        let tool = inv
            .args
            .iter()
            .find(|a| a.contains("diff-args"))
            .expect("tool args");
        assert!(tool.contains("--diff-algorithm"));
        assert!(tool.contains("myers"));
        assert!(tool.contains("--scrolloff"));
        assert!(tool.contains("5"));
        assert!(tool.contains("--context-lines"));
        assert!(tool.contains("9"));
    }

    #[test]
    fn resolve_uses_merge_args() {
        let args = ResolveArgs { paths: vec![] };
        let inv = resolve(&program(), &args);
        assert_eq!(inv.args[5], "resolve");
        assert!(inv.args.iter().any(|a| a.contains("merge-args")));
        assert!(contains_pair(&inv.args, "--tool", "oyui"));
    }

    #[test]
    fn squash_uses_edit_args_and_forwards_options() {
        let args = SquashArgs {
            from: Some("@".into()),
            into: Some("@-".into()),
            keep_emptied: true,
            paths: vec![],
            view: crate::cli::ViewArgs {
                diff_algorithm: crate::cli::DiffAlgorithm::Histogram,
                scrolloff: 2,
                context_lines: 4,
            },
        };
        let inv = squash(&program(), &args);
        assert_eq!(inv.args[5], "squash");
        assert!(inv.args.iter().any(|a| a.contains("edit-args")));
        assert!(contains_pair(&inv.args, "--from", "@"));
        assert!(contains_pair(&inv.args, "--into", "@-"));
        assert!(inv.args.contains(&"--keep-emptied".to_string()));
    }

    #[test]
    fn interdiff_forwards_from_and_to() {
        let args = InterdiffArgs {
            from: Some("@-".into()),
            to: Some("@".into()),
            paths: vec![],
            ..Default::default()
        };
        let inv = interdiff(&program(), &args);
        assert_eq!(inv.args[5], "interdiff");
        assert!(contains_pair(&inv.args, "--from", "@-"));
        assert!(contains_pair(&inv.args, "--to", "@"));
    }
}
