//! External command construction and execution for VCS backends.
use std::path::PathBuf;
use std::process::{Command, ExitStatus};

/// The oyui executable plus the config to forward to nested tool invocations.
#[derive(Debug, Clone)]
pub struct ToolProgram {
    pub exe: PathBuf,
    pub config: Option<PathBuf>,
}

impl ToolProgram {
    /// The `oyui` arguments that precede the tool subcommand for a nested call.
    pub fn prefix_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(config) = &self.config {
            args.push("--config".to_string());
            args.push(config.to_string_lossy().into_owned());
        }
        args
    }
}

/// A fully resolved external command to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInvocation {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl ToolInvocation {
    /// Runs the command, inheriting stdio, and returns its exit status.
    pub fn run(&self) -> std::io::Result<ExitStatus> {
        Command::new(&self.program).args(&self.args).status()
    }
}

/// Renders `s` as a TOML basic string literal, quotes included.
pub fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// POSIX single-quotes `s` so it survives a shell round-trip.
pub fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_string_escapes_specials() {
        assert_eq!(toml_string(r#"a"b\c"#), r#""a\"b\\c""#);
    }

    #[test]
    fn shell_quote_wraps_and_escapes() {
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }
}
