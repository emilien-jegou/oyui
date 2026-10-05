//! Git/jj conflict-marker parsing and resolution.
//!
//! Conflicted files are materialized with markers:
//!
//! ```text
//! <<<<<<< ours
//! ours line
//! ||||||| base      (diff3 style only)
//! base line
//! =======
//! theirs line
//! >>>>>>> theirs
//! ```

/// True when `line` is a conflict marker (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`).
pub fn is_marker_line(line: &str) -> bool {
    line.starts_with("<<<<<<<")
        || line.starts_with("|||||||")
        || line.starts_with("=======")
        || line.starts_with(">>>>>>>")
}

/// How a single conflict should be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Ours,
    Theirs,
    Both,
}

impl Side {
    /// Parses a side token (`ours`, `theirs`, `both`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ours" | "left" | "local" => Some(Side::Ours),
            "theirs" | "right" | "remote" => Some(Side::Theirs),
            "both" => Some(Side::Both),
            _ => None,
        }
    }
}

/// A conflicting region between the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub ours: Vec<String>,
    pub base: Option<Vec<String>>,
    pub theirs: Vec<String>,
}

impl Conflict {
    /// Number of lines the conflict occupies when left unresolved.
    pub fn marker_lines(&self) -> usize {
        let base = self.base.as_ref().map_or(0, |b| b.len() + 1);
        self.ours.len() + self.theirs.len() + base + 3
    }

    /// Returns the lines for a chosen side.
    pub fn resolve(&self, side: Side) -> Vec<String> {
        match side {
            Side::Ours => self.ours.clone(),
            Side::Theirs => self.theirs.clone(),
            Side::Both => {
                let mut out = self.ours.clone();
                out.extend(self.theirs.iter().cloned());
                out
            }
        }
    }
}

/// One part of a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Common(Vec<String>),
    Conflict(Conflict),
}

/// A file split into shared and conflicting regions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConflictedFile {
    pub segments: Vec<Segment>,
}

impl ConflictedFile {
    /// True when `content` contains at least one conflict marker.
    pub fn is_conflicted(content: &str) -> bool {
        content
            .lines()
            .any(|l| l.starts_with("<<<<<<<") && l.len() >= 7)
    }

    /// Parses `content`, returning `None` when there are no markers.
    pub fn parse(content: &str) -> Option<Self> {
        if !Self::is_conflicted(content) {
            return None;
        }

        let mut segments = Vec::new();
        let mut current: Vec<String> = Vec::new();

        // Parser state: `None` = common, `Some` = inside a conflict.
        let mut conflict: Option<Conflict> = None;
        let mut section = Section::Ours;

        for line in content.lines() {
            if line.starts_with("<<<<<<<") {
                if !current.is_empty() {
                    segments.push(Segment::Common(std::mem::take(&mut current)));
                }
                conflict = Some(Conflict {
                    ours: Vec::new(),
                    base: None,
                    theirs: Vec::new(),
                });
                section = Section::Ours;
            } else if line.starts_with("|||||||") && conflict.is_some() {
                section = Section::Base;
                if let Some(c) = conflict.as_mut() {
                    c.base.get_or_insert_with(Vec::new);
                }
            } else if line.starts_with("=======") && conflict.is_some() {
                section = Section::Theirs;
            } else if line.starts_with(">>>>>>>") && conflict.is_some() {
                if let Some(c) = conflict.take() {
                    segments.push(Segment::Conflict(c));
                }
                section = Section::Ours;
            } else {
                match (conflict.as_mut(), section) {
                    (Some(c), Section::Ours) => c.ours.push(line.to_string()),
                    (Some(c), Section::Base) => {
                        c.base.get_or_insert_with(Vec::new).push(line.to_string())
                    }
                    (Some(c), Section::Theirs) => c.theirs.push(line.to_string()),
                    (None, _) => current.push(line.to_string()),
                }
            }
        }

        // An unterminated conflict: treat the remainder as ours.
        if let Some(c) = conflict.take() {
            segments.push(Segment::Conflict(c));
        }
        if !current.is_empty() {
            segments.push(Segment::Common(current));
        }

        Some(Self { segments })
    }

    /// Number of conflicting regions.
    pub fn conflict_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| matches!(s, Segment::Conflict(_)))
            .count()
    }

    /// Resolves every conflict with the same side.
    pub fn resolve_all(&self, side: Side) -> String {
        self.resolve(&vec![side; self.conflict_count()])
    }

    /// Builds the on-screen content: conflicts are shown in full, or collapsed
    /// to a single summary marker line when folded.
    ///
    /// This never touches the file on disk — it only drives rendering.
    pub fn display(&self, folded: &[bool], choices: &[Option<Side>]) -> String {
        let mut lines = Vec::new();
        let mut idx = 0;
        for segment in &self.segments {
            match segment {
                Segment::Common(text) => lines.extend(text.iter().cloned()),
                Segment::Conflict(c) => {
                    if folded.get(idx).copied().unwrap_or(false) {
                        lines.push(summary_line(choices.get(idx).copied().flatten()));
                    } else {
                        write_markers(&mut lines, c);
                    }
                    idx += 1;
                }
            }
        }
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// Line ranges of each conflict in [`display`](Self::display) output.
    pub fn display_ranges(&self, folded: &[bool]) -> Vec<std::ops::Range<usize>> {
        let mut ranges = Vec::new();
        let mut line = 0usize;
        let mut idx = 0usize;
        for segment in &self.segments {
            match segment {
                Segment::Common(text) => line += text.len(),
                Segment::Conflict(c) => {
                    let len = if folded.get(idx).copied().unwrap_or(false) {
                        1
                    } else {
                        c.marker_lines()
                    };
                    ranges.push(line..line + len);
                    line += len;
                    idx += 1;
                }
            }
        }
        ranges
    }

    /// Resolves conflicts using `choices` (one per conflict, in order).
    ///
    /// Missing choices leave the conflict markers in place.
    pub fn resolve(&self, choices: &[Side]) -> String {
        let resolved: Vec<Option<Side>> = choices.iter().map(|s| Some(*s)).collect();
        self.resolve_optional(&resolved)
    }

    /// Resolves with per-conflict optional choices; `None` keeps the markers.
    pub fn resolve_optional(&self, choices: &[Option<Side>]) -> String {
        let mut lines = Vec::new();
        let mut idx = 0;
        for segment in &self.segments {
            match segment {
                Segment::Common(text) => lines.extend(text.iter().cloned()),
                Segment::Conflict(c) => {
                    match choices.get(idx).copied().flatten() {
                        Some(side) => lines.extend(c.resolve(side)),
                        None => write_markers(&mut lines, c),
                    }
                    idx += 1;
                }
            }
        }
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// A single line summarising a folded conflict and its current choice.
fn summary_line(choice: Option<Side>) -> String {
    let label = match choice {
        Some(Side::Ours) => "ours",
        Some(Side::Theirs) => "theirs",
        Some(Side::Both) => "both",
        None => "unresolved",
    };
    format!("<<<<<<< {label} ⋯ >>>>>>>")
}

/// Emits the unresolved marker block for a conflict.
fn write_markers(lines: &mut Vec<String>, conflict: &Conflict) {
    lines.push("<<<<<<< ours".to_string());
    lines.extend(conflict.ours.iter().cloned());
    if let Some(base) = &conflict.base {
        lines.push("||||||| base".to_string());
        lines.extend(base.iter().cloned());
    }
    lines.push("=======".to_string());
    lines.extend(conflict.theirs.iter().cloned());
    lines.push(">>>>>>> theirs".to_string());
}

#[derive(Clone, Copy)]
enum Section {
    Ours,
    Base,
    Theirs,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFLICTED: &str = "\
fn main() {
<<<<<<< ours
    let x = 1;
||||||| base
    let x = 0;
=======
    let x = 2;
>>>>>>> theirs
    println!(\"{x}\");
}
";

    #[test]
    fn detects_and_parses_conflicts() {
        assert!(ConflictedFile::is_conflicted(CONFLICTED));
        let parsed = ConflictedFile::parse(CONFLICTED).expect("conflicted");
        assert_eq!(parsed.conflict_count(), 1);

        let conflict = parsed
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(conflict.ours, vec!["    let x = 1;"]);
        assert_eq!(
            conflict.base.as_deref(),
            Some(&["    let x = 0;".to_string()][..])
        );
        assert_eq!(conflict.theirs, vec!["    let x = 2;"]);
    }

    #[test]
    fn clean_files_are_not_conflicted() {
        assert!(!ConflictedFile::is_conflicted("just\ntext\n"));
        assert!(ConflictedFile::parse("just\ntext\n").is_none());
    }

    #[test]
    fn resolves_each_side() {
        let parsed = ConflictedFile::parse(CONFLICTED).unwrap();
        let ours = parsed.resolve_all(Side::Ours);
        assert!(ours.contains("let x = 1;"));
        assert!(!ours.contains("let x = 2;"));
        assert!(!ours.contains("<<<<<<<"));

        let theirs = parsed.resolve_all(Side::Theirs);
        assert!(theirs.contains("let x = 2;"));

        let both = parsed.resolve_all(Side::Both);
        assert!(both.contains("let x = 1;") && both.contains("let x = 2;"));
    }

    #[test]
    fn unresolved_conflicts_keep_markers() {
        let parsed = ConflictedFile::parse(CONFLICTED).unwrap();
        let out = parsed.resolve(&[]);
        assert!(out.contains("<<<<<<< ours"));
        assert!(out.contains(">>>>>>> theirs"));
    }

    #[test]
    fn resolve_optional_keeps_unchosen_markers() {
        let two = "\
a
<<<<<<< ours
1
=======
2
>>>>>>> theirs
b
<<<<<<< ours
3
=======
4
>>>>>>> theirs
c
";
        let parsed = ConflictedFile::parse(two).expect("conflicted");
        assert_eq!(parsed.conflict_count(), 2);

        let out = parsed.resolve_optional(&[Some(Side::Ours), None]);
        assert!(out.contains('1') && !out.contains('2'));
        assert!(out.contains("<<<<<<< ours"));
        assert!(out.contains('3') && out.contains('4'));
    }

    #[test]
    fn side_tokens_parse() {
        assert_eq!(Side::parse("ours"), Some(Side::Ours));
        assert_eq!(Side::parse("LOCAL"), Some(Side::Ours));
        assert_eq!(Side::parse("theirs"), Some(Side::Theirs));
        assert_eq!(Side::parse("both"), Some(Side::Both));
        assert_eq!(Side::parse("nope"), None);
    }
}
