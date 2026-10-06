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

/// True when `line` is a conflict marker (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`,
/// plus jj native `%%%%%%%`, `+++++++`/`-------` snapshots and `\\\\\\\` continuations).
pub fn is_marker_line(line: &str) -> bool {
    line.starts_with("<<<<<<<")
        || line.starts_with("|||||||")
        || line.starts_with("=======")
        || line.starts_with(">>>>>>>")
        || marker_run(line, '%').is_some_and(|n| n >= 7)
        || marker_run(line, '+').is_some_and(|n| n >= 7)
        || marker_run(line, '-').is_some_and(|n| n >= 7 && is_jj_base_header(line))
        || marker_run(line, '\\').is_some_and(|n| n >= 7)
}

/// Length of the leading run of `ch` at the start of `line`.
fn marker_run(line: &str, ch: char) -> Option<usize> {
    let n = line.chars().take_while(|&c| c == ch).count();
    (n > 0).then_some(n)
}

/// True for jj snapshot base headers (`------- <label>`), as opposed to a
/// unified-diff removed line (`-text`) which only has a single `-`.
fn is_jj_base_header(line: &str) -> bool {
    let n = line.chars().take_while(|&c| c == '-').count();
    n >= 7 && (line[n..].starts_with([' ', '\t']) || line[n..].is_empty())
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
///
/// `ours`/`base`/`theirs` hold the stripped side contents used for
/// resolution. For jj native blocks (`%%%%%%%`/`+++++++`/`-------`),
/// `jj_raw` additionally preserves the verbatim marker block — including
/// jj's labels (commit ids, `diff from:`/`to:`) — so the UI and confirm
/// write-back keep jj's representation instead of converting to git style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub ours: Vec<String>,
    pub base: Option<Vec<String>>,
    pub theirs: Vec<String>,
    pub jj_raw: Option<JjRaw>,
}

/// Verbatim jj native conflict block plus its side layout.
///
/// All ranges are body coordinates (indices into `body`, excluding the
/// `<<<<<<<`/`>>>>>>>` lines). `side1` covers side one's section including
/// its headers; `side1_content` covers only side one's content lines
/// (headers and diff-removed lines excluded) for folded display; `sep`
/// is the side-two snapshot header; `side2` covers side-two content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JjRaw {
    pub start: String,
    pub end: String,
    pub body: Vec<String>,
    pub side1: std::ops::Range<usize>,
    pub side1_content: Vec<std::ops::Range<usize>>,
    pub sep: usize,
    pub side2: std::ops::Range<usize>,
}

impl Conflict {
    /// Number of lines the conflict occupies when left unresolved.
    pub fn marker_lines(&self) -> usize {
        if let Some(raw) = &self.jj_raw {
            return raw.body.len() + 2;
        }
        let base = self.base.as_ref().map_or(0, |b| b.len() + 1);
        self.ours.len() + self.theirs.len() + base + 3
    }

    /// True for jj native blocks (preserved verbatim, not git-converted).
    pub fn is_jj(&self) -> bool {
        self.jj_raw.is_some()
    }

    /// The unresolved marker block: verbatim jj lines for jj conflicts,
    /// git markers otherwise.
    pub fn unresolved_block(&self) -> Vec<String> {
        if let Some(raw) = &self.jj_raw {
            let mut out = Vec::with_capacity(raw.body.len() + 2);
            out.push(raw.start.clone());
            out.extend(raw.body.iter().cloned());
            out.push(raw.end.clone());
            return out;
        }
        let mut out = Vec::new();
        write_markers(&mut out, self);
        out
    }

    /// Absolute `(ours, sep, theirs)` layout for `abs_start`-based side
    /// mapping. For jj this follows the raw block: side-one section maps
    /// to ours, the side-two snapshot header and below to theirs (mirroring
    /// the git rule that the base block maps to ours).
    pub fn layout_ranges(
        &self,
        abs_start: usize,
    ) -> (std::ops::Range<usize>, usize, std::ops::Range<usize>) {
        if let Some(raw) = &self.jj_raw {
            let shift = |r: &std::ops::Range<usize>| abs_start + 1 + r.start..abs_start + 1 + r.end;
            return (
                shift(&raw.side1),
                abs_start + 1 + raw.sep,
                shift(&raw.side2),
            );
        }
        let ours_start = abs_start + 1;
        let ours_end = ours_start + self.ours.len();
        let base_len = self.base.as_ref().map(|b| b.len() + 1).unwrap_or(0);
        let sep = ours_end + base_len;
        let theirs_start = sep + 1;
        let theirs_end = theirs_start + self.theirs.len();
        (ours_start..ours_end, sep, theirs_start..theirs_end)
    }

    /// Absolute kept ranges for `choice` in canonical (on-disk) coordinates.
    ///
    /// For jj these are the raw side content lines (prefixes preserved), so
    /// folded conflicts keep showing jj's representation; for git they are
    /// the side ranges. `None` keeps nothing.
    pub fn kept_ranges(
        &self,
        abs_start: usize,
        choice: Option<Side>,
    ) -> Vec<std::ops::Range<usize>> {
        let shift = |r: &std::ops::Range<usize>| abs_start + 1 + r.start..abs_start + 1 + r.end;
        match (&self.jj_raw, choice) {
            (Some(raw), Some(Side::Ours)) => raw.side1_content.iter().map(shift).collect(),
            (Some(raw), Some(Side::Theirs)) => vec![shift(&raw.side2)],
            (Some(raw), Some(Side::Both)) => {
                let mut out: Vec<std::ops::Range<usize>> =
                    raw.side1_content.iter().map(shift).collect();
                out.push(shift(&raw.side2));
                out
            }
            (Some(_), None) => Vec::new(),
            (None, Some(Side::Ours)) => {
                let (ours, _, _) = self.layout_ranges(abs_start);
                vec![ours]
            }
            (None, Some(Side::Theirs)) => {
                let (_, _, theirs) = self.layout_ranges(abs_start);
                vec![theirs]
            }
            (None, Some(Side::Both)) => {
                let (ours, _, theirs) = self.layout_ranges(abs_start);
                vec![ours, theirs]
            }
            (None, None) => Vec::new(),
        }
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
    ///
    /// Understands git (`<<<<<<<`/`|||||||`/`=======`/`>>>>>>>`) and jj
    /// native styles (`ui.conflict-marker-style = "diff"` with `%%%%%%%`
    /// diffs + `+++++++` snapshots, and `"snapshot"` with `+++++++` sides
    /// plus `-------` base). jj conflicts normalize to git markers on
    /// display/write-back, which jj re-parses on confirm.
    pub fn parse(content: &str) -> Option<Self> {
        if !Self::is_conflicted(content) {
            return None;
        }

        let mut segments = Vec::new();
        let mut current: Vec<String> = Vec::new();

        // Parser state: `None` = common, `Some` = inside a conflict.
        let mut conflict: Option<Conflict> = None;
        let mut section = Section::Ours;
        // Raw inner lines of the current block, used to detect jj style.
        let mut block_raw: Vec<String> = Vec::new();
        let mut block_start = String::new();

        for line in content.lines() {
            if line.starts_with("<<<<<<<") {
                if !current.is_empty() {
                    segments.push(Segment::Common(std::mem::take(&mut current)));
                }
                conflict = Some(Conflict {
                    ours: Vec::new(),
                    base: None,
                    theirs: Vec::new(),
                    jj_raw: None,
                });
                section = Section::Ours;
                block_raw.clear();
                block_start = line.to_string();
            } else if line.starts_with(">>>>>>>") && conflict.is_some() {
                // If the block used jj markers, re-parse its body natively,
                // preserving the verbatim block (labels included).
                if let Some(c) = parse_jj_block(&block_start, &block_raw, line) {
                    segments.push(Segment::Conflict(c));
                    conflict = None;
                } else if let Some(c) = conflict.take() {
                    segments.push(Segment::Conflict(c));
                }
                block_raw.clear();
                section = Section::Ours;
            } else if conflict.is_some() {
                block_raw.push(line.to_string());
                if line.starts_with("|||||||") {
                    section = Section::Base;
                    if let Some(c) = conflict.as_mut() {
                        c.base.get_or_insert_with(Vec::new);
                    }
                } else if line.starts_with("=======") {
                    // A bare `=======` at column 0 is the git separator, but
                    // only when this is not a jj block (jj blocks are
                    // re-parsed above; guard against ` =======` content).
                    if !block_looks_jj(&block_raw) {
                        section = Section::Theirs;
                    } else {
                        push_git_line(conflict.as_mut(), section, line);
                    }
                } else {
                    push_git_line(conflict.as_mut(), section, line);
                }
            } else {
                current.push(line.to_string());
            }
        }

        // An unterminated conflict: try jj first, else treat as git ours.
        if conflict.is_some() {
            if let Some(c) = parse_jj_block(&block_start, &block_raw, ">>>>>>>") {
                segments.push(Segment::Conflict(c));
            } else if let Some(c) = conflict.take() {
                segments.push(Segment::Conflict(c));
            }
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

    /// Builds the on-screen content: unfolded conflicts show markers; folded
    /// ones show a header framing the chosen side's lines plus a footer, so
    /// the choice stays visible and editable while markers, base and the
    /// losing side stay hidden.
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
                        lines.push(header_line(choices.get(idx).copied().flatten()));
                        if let Some(side) = choices.get(idx).copied().flatten() {
                            lines.extend(c.resolve(side));
                        }
                        lines.push(footer_line());
                    } else {
                        lines.extend(c.unresolved_block());
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

    /// Renders the file with jj snapshot-style markers (`ui.conflict-marker-style
    /// = "snapshot"): each conflict becomes a `<<<<<<<` block with verbatim
    /// `+++++++` sides and a `-------` base, preserving jj's representation
    /// instead of converting to git markers.
    ///
    /// Used when a merge target must be synthesized from snapshots (e.g. jj
    /// mergetool hands over marker-less `$base`/`$left`/`$right`); common
    /// segments pass through untouched.
    pub fn to_jj_snapshot(&self) -> String {
        let total = self.conflict_count();
        let mut lines = Vec::new();
        let mut idx = 0;
        for segment in &self.segments {
            match segment {
                Segment::Common(text) => lines.extend(text.iter().cloned()),
                Segment::Conflict(c) => {
                    idx += 1;
                    lines.push(format!("<<<<<<< conflict {idx} of {total}"));
                    lines.push("+++++++ left".to_string());
                    lines.extend(c.ours.iter().cloned());
                    if let Some(base) = &c.base {
                        lines.push("------- base".to_string());
                        lines.extend(base.iter().cloned());
                    }
                    lines.push("+++++++ right".to_string());
                    lines.extend(c.theirs.iter().cloned());
                    lines.push(format!(">>>>>>> conflict {idx} of {total} ends"));
                }
            }
        }
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// New-content line ranges of each conflict, fully expanded.
    pub fn marker_ranges(&self) -> Vec<std::ops::Range<usize>> {
        self.display_ranges(&vec![false; self.conflict_count()])
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
    ///
    /// Merge output is driven by choices alone: conflict hunks are not
    /// stageable, so staging selections never affect the written resolution.
    pub fn resolve_optional(&self, choices: &[Option<Side>]) -> String {
        let mut lines = Vec::new();
        let mut idx = 0;
        for segment in &self.segments {
            match segment {
                Segment::Common(text) => lines.extend(text.iter().cloned()),
                Segment::Conflict(c) => {
                    match choices.get(idx).copied().flatten() {
                        Some(side) => lines.extend(c.resolve(side)),
                        None => lines.extend(c.unresolved_block()),
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

/// Header framing a folded conflict's chosen lines (full-width, no file line).
/// Returns the bare side label; the frame row renders it as `label ———…`.
pub fn header_line(choice: Option<Side>) -> String {
    match choice {
        Some(Side::Ours) => "ours",
        Some(Side::Theirs) => "theirs",
        Some(Side::Both) => "all",
        None => "unresolved",
    }
    .to_string()
}

/// Footer closing a folded conflict's frame (full-width, no file line).
/// Empty: the frame row renders a bare `———…` rule.
pub fn footer_line() -> String {
    String::new()
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

/// Pushes a non-marker line into the in-progress git conflict.
fn push_git_line(conflict: Option<&mut Conflict>, section: Section, line: &str) {
    match (conflict, section) {
        (Some(c), Section::Ours) => c.ours.push(line.to_string()),
        (Some(c), Section::Base) => c.base.get_or_insert_with(Vec::new).push(line.to_string()),
        (Some(c), Section::Theirs) => c.theirs.push(line.to_string()),
        (None, _) => {}
    }
}

/// True when the collected block body contains jj section markers, i.e. a
/// `%%%%%%%` diff header or a `+++++++`/`-------` snapshot header.
fn block_looks_jj(block_raw: &[String]) -> bool {
    block_raw.iter().any(|l| {
        marker_run(l, '%').is_some_and(|n| n >= 7)
            || marker_run(l, '+').is_some_and(|n| n >= 7)
            || (marker_run(l, '-').is_some_and(|n| n >= 7) && is_jj_base_header(l))
    })
}

/// Parses a jj native conflict body (lines between `<<<<<<<` and `>>>>>>>`),
/// preserving the verbatim block (labels included) in the result.
///
/// Returns `None` when the body has no jj section markers (plain git block).
/// Diff sections (`%%%%%%%`, with `\\\\\\\` label continuations skipped)
/// contribute context+added lines to that side and context+removed lines to
/// the base; snapshot sections (`+++++++`) are verbatim sides and `-------`
/// is the verbatim base. With more than two sides, extras fold into
/// `theirs` so `Both` resolution still contains everything.
fn parse_jj_block(start: &str, block_raw: &[String], end: &str) -> Option<Conflict> {
    if !block_looks_jj(block_raw) {
        return None;
    }
    enum Kind {
        Diff,
        Snap,
        Base,
    }
    struct Section {
        kind: Kind,
        header: usize,
        /// Stripped content lines for resolution.
        new: Vec<String>,
        old: Vec<String>,
        /// Body indices of content lines kept when this side is folded.
        keep: Vec<usize>,
        /// Body end (exclusive) of the section including its content.
        end: usize,
    }
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;
    let flush = |current: &mut Option<Section>, sections: &mut Vec<Section>| {
        if let Some(s) = current.take() {
            sections.push(s);
        }
    };
    for (i, line) in block_raw.iter().enumerate() {
        if marker_run(line, '%').is_some_and(|n| n >= 7) {
            flush(&mut current, &mut sections);
            current = Some(Section {
                kind: Kind::Diff,
                header: i,
                new: Vec::new(),
                old: Vec::new(),
                keep: Vec::new(),
                end: i + 1,
            });
        } else if marker_run(line, '\\').is_some_and(|n| n >= 7) {
            // `\\\\\\\ to:` label continuation of a diff header.
            if let Some(s) = current.as_mut() {
                s.end = i + 1;
            }
        } else if marker_run(line, '+').is_some_and(|n| n >= 7) {
            flush(&mut current, &mut sections);
            current = Some(Section {
                kind: Kind::Snap,
                header: i,
                new: Vec::new(),
                old: Vec::new(),
                keep: Vec::new(),
                end: i + 1,
            });
        } else if marker_run(line, '-').is_some_and(|n| n >= 7) && is_jj_base_header(line) {
            flush(&mut current, &mut sections);
            current = Some(Section {
                kind: Kind::Base,
                header: i,
                new: Vec::new(),
                old: Vec::new(),
                keep: Vec::new(),
                end: i + 1,
            });
        } else if line.starts_with("|||||||") || line.starts_with("=======") {
            // Mixed git markers inside a jj block: let the git path own it.
            return None;
        } else {
            match current.as_mut() {
                Some(s) => {
                    s.end = i + 1;
                    match s.kind {
                        Kind::Diff => {
                            if let Some(body) = line.strip_prefix(' ') {
                                s.new.push(body.to_string());
                                s.old.push(body.to_string());
                                s.keep.push(i);
                            } else if let Some(body) = line.strip_prefix('+') {
                                // `+++++++` headers handled above; single `+` is added.
                                s.new.push(body.to_string());
                                s.keep.push(i);
                            } else if let Some(body) = line.strip_prefix('-') {
                                s.old.push(body.to_string());
                            } else if line.starts_with('\\') {
                                // `\ No newline at end of file` metadata.
                            } else {
                                // Header-less/empty lines are verbatim on both.
                                s.new.push(line.clone());
                                s.old.push(line.clone());
                                s.keep.push(i);
                            }
                        }
                        Kind::Snap => {
                            s.new.push(line.clone());
                            s.keep.push(i);
                        }
                        Kind::Base => {
                            s.old.push(line.clone());
                        }
                    }
                }
                None => {
                    // Content before any header: implicit diff section.
                    let mut s = Section {
                        kind: Kind::Diff,
                        header: i,
                        new: Vec::new(),
                        old: Vec::new(),
                        keep: Vec::new(),
                        end: i + 1,
                    };
                    if let Some(body) = line.strip_prefix(' ') {
                        s.new.push(body.to_string());
                        s.old.push(body.to_string());
                        s.keep.push(i);
                    } else if let Some(body) = line.strip_prefix('+') {
                        s.new.push(body.to_string());
                        s.keep.push(i);
                    } else if let Some(body) = line.strip_prefix('-') {
                        s.old.push(body.to_string());
                    } else {
                        s.new.push(line.clone());
                        s.old.push(line.clone());
                        s.keep.push(i);
                    }
                    current = Some(s);
                }
            }
        }
    }
    flush(&mut current, &mut sections);

    // First non-base section is side one; last snapshot is side two.
    let side1_idx = sections
        .iter()
        .position(|s| !matches!(s.kind, Kind::Base))?;
    let side2_idx = sections
        .iter()
        .rposition(|s| matches!(s.kind, Kind::Snap))
        .filter(|&j| {
            // A snapshot side needs a distinct side one; a lone snapshot
            // without any diff is not a resolvable two-sided conflict.
            sections[..j].iter().any(|s| !matches!(s.kind, Kind::Base))
        });
    // Stripped side contents for resolution; extras fold into theirs.
    let mut extra: Vec<String> = Vec::new();
    let mut base: Option<Vec<String>> = None;
    for (j, s) in sections.iter().enumerate() {
        if j == side1_idx {
            continue;
        }
        if Some(j) == side2_idx {
            continue;
        }
        match &s.kind {
            Kind::Diff => {
                if base.is_none() {
                    base = Some(s.old.clone());
                }
                extra.extend(s.new.iter().cloned());
            }
            Kind::Snap => extra.extend(s.new.iter().cloned()),
            Kind::Base => {
                if base.is_none() {
                    base = Some(s.old.clone());
                } else {
                    extra.extend(s.old.iter().cloned());
                }
            }
        }
    }
    let s1 = &sections[side1_idx];
    if base.is_none() {
        // The base is side one's diff-old, or the explicit `-------` base.
        base = Some(match &s1.kind {
            Kind::Diff => s1.old.clone(),
            Kind::Snap | Kind::Base => sections
                .iter()
                .find_map(|s| match &s.kind {
                    Kind::Base => Some(s.old.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        });
    }
    let ours = s1.new.clone();
    let side1_span = s1.header..s1.end;
    let side1_content = ranges_of(&s1.keep);
    let (theirs, sep, side2) = match side2_idx {
        Some(j) => {
            let s2 = &sections[j];
            let mut t = s2.new.clone();
            t.extend(extra.iter().cloned());
            let content_start = s2.header + 1;
            (t, s2.header, content_start..s2.end)
        }
        None => (extra, block_raw.len(), block_raw.len()..block_raw.len()),
    };
    Some(Conflict {
        ours,
        base,
        theirs,
        jj_raw: Some(JjRaw {
            start: start.to_string(),
            end: end.to_string(),
            body: block_raw.to_vec(),
            side1: side1_span,
            side1_content,
            sep,
            side2,
        }),
    })
}

/// Merges sorted body indices into contiguous ranges.
fn ranges_of(indices: &[usize]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut iter = indices.iter().peekable();
    while let Some(&s) = iter.next() {
        let mut e = s + 1;
        while iter.peek().is_some_and(|&&n| n == e) {
            iter.next();
            e += 1;
        }
        out.push(s..e);
    }
    out
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

    const JJ_DIFF_STYLE: &str = "\
<<<<<<< conflict 1 of 1
%%%%%%% diff from: vpxusssl 38d49363 \"merge base\"
\\\\\\\\\\\\\\        to: rtsqusxu 2768b0b9 \"commit A\"
 apple
-grape
+grapefruit
 orange
+++++++ ysrnknol 7a20f389 \"commit B\"
APPLE
GRAPE
ORANGE
>>>>>>> conflict 1 of 1 ends
";

    #[test]
    fn parses_jj_diff_style() {
        assert!(ConflictedFile::is_conflicted(JJ_DIFF_STYLE));
        let parsed = ConflictedFile::parse(JJ_DIFF_STYLE).expect("jj conflicted");
        assert_eq!(parsed.conflict_count(), 1);
        let c = parsed
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(c.ours, vec!["apple", "grapefruit", "orange"]);
        assert_eq!(
            c.base.as_deref(),
            Some(
                &[
                    "apple".to_string(),
                    "grape".to_string(),
                    "orange".to_string()
                ][..]
            )
        );
        assert_eq!(c.theirs, vec!["APPLE", "GRAPE", "ORANGE"]);
        assert!(is_marker_line("%%%%%%% diff from: x"));
        assert!(is_marker_line("+++++++ abc123 \"side\""));
        // Unresolved output preserves the verbatim jj block (labels included),
        // never converting to git markers.
        let out = parsed.resolve_optional(&[None]);
        assert_eq!(out, JJ_DIFF_STYLE);
        assert!(c.is_jj());
        // marker_lines covers the raw block: start + 10 body + end.
        assert_eq!(c.marker_lines(), 12);
        assert_eq!(parsed.marker_ranges(), vec![0..12]);
        // Resolved sides are stripped (no +/- prefixes, no headers).
        assert_eq!(
            parsed.resolve_all(Side::Ours),
            "apple\ngrapefruit\norange\n"
        );
        assert_eq!(parsed.resolve_all(Side::Theirs), "APPLE\nGRAPE\nORANGE\n");
        // Layout: diff section maps to ours, snapshot header and below theirs.
        let (ours, sep, theirs) = c.layout_ranges(0);
        assert_eq!(ours, 1..7);
        assert_eq!(sep, 7);
        assert_eq!(theirs, 8..11);
        // Folded-kept lines are the raw side content (diff headers and the
        // removed line excluded, +/- prefixes preserved).
        assert_eq!(c.kept_ranges(0, Some(Side::Ours)), vec![3..4, 5..7]);
        assert_eq!(c.kept_ranges(0, Some(Side::Theirs)), vec![8..11]);
    }

    const JJ_SNAPSHOT_STYLE: &str = "\
<<<<<<< conflict 1 of 1
+++++++ rtsqusxu 2768b0b9 \"side A\"
apple
grapefruit
------- vpxusssl 38d49363 \"merge base\"
apple
grape
+++++++ ysrnknol 7a20f389 \"side B\"
APPLE
>>>>>>> conflict 1 of 1 ends
";

    #[test]
    fn jj_snapshot_synthesis_round_trips() {
        let parsed = ConflictedFile::parse(CONFLICTED).expect("conflicted");
        let snap = parsed.to_jj_snapshot();
        assert!(snap.contains("+++++++ left"));
        assert!(snap.contains("------- base"));
        assert!(snap.contains("+++++++ right"));
        assert!(!snap.contains("======="));
        let reparsed = ConflictedFile::parse(&snap).expect("snapshot parses");
        assert_eq!(reparsed.conflict_count(), 1);
        let c = reparsed
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert!(c.is_jj());
        assert_eq!(c.ours, vec!["    let x = 1;"]);
        assert_eq!(c.theirs, vec!["    let x = 2;"]);
        assert_eq!(c.base.as_deref(), Some(&["    let x = 0;".to_string()][..]));
        // Unresolved write-back preserves the synthesized jj block.
        assert_eq!(reparsed.resolve_optional(&[None]), snap);
    }

    #[test]
    fn parses_minimal_jj_conflict_like_fruits_txt() {
        let content = concat!(
            "\n\n<<<<<<< conflict 1 of 1\n",
            "%%%%%%% diff from: lnoqwuzl d6e5c7ff\n",
            "\\\\\\\\\\\\\\        to: kzvlkwvs bba6ee73\n",
            "-Hello\n+Halo\n",
            "+++++++ ruqyqyzm 84301dd8\n",
            "Good morning\n",
            ">>>>>>> conflict 1 of 1 ends\n\nWarld\n",
        );
        let parsed = ConflictedFile::parse(content).expect("jj conflicted");
        assert_eq!(parsed.conflict_count(), 1);
        let c = parsed
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert!(c.is_jj());
        assert_eq!(c.ours, vec!["Halo"]);
        assert_eq!(c.base.as_deref(), Some(&["Hello".to_string()][..]));
        assert_eq!(c.theirs, vec!["Good morning"]);
        // Verbatim round-trip: labels and structure preserved, no git markers.
        let out = parsed.resolve_optional(&[None]);
        assert_eq!(out, content);
        assert!(!out.contains("|||||||") && !out.contains("======="));
        assert_eq!(parsed.resolve_all(Side::Ours), "\n\nHalo\n\nWarld\n");
        assert_eq!(
            parsed.resolve_all(Side::Theirs),
            "\n\nGood morning\n\nWarld\n"
        );
        // Cursor mapping: diff section (incl. headers) -> ours, snapshot -> theirs.
        let (ours, sep, theirs) = c.layout_ranges(2);
        assert_eq!(ours, 3..7);
        assert_eq!(sep, 7);
        assert_eq!(theirs, 8..9);
    }

    #[test]
    fn parses_jj_snapshot_style() {
        let parsed = ConflictedFile::parse(JJ_SNAPSHOT_STYLE).expect("jj snapshot");
        assert_eq!(parsed.conflict_count(), 1);
        let c = parsed
            .segments
            .iter()
            .find_map(|s| match s {
                Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(c.ours, vec!["apple", "grapefruit"]);
        assert_eq!(
            c.base.as_deref(),
            Some(&["apple".to_string(), "grape".to_string()][..])
        );
        assert_eq!(c.theirs, vec!["APPLE"]);
    }
}
