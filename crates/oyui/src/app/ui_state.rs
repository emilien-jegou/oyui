//! Main-task-confined UI state behind one lock shared by cloned handlers.
//!
//! The dispatch ABI (`BoxedHandler` is `Arc`-shared with the script host)
//! forces interior mutability, so all ephemeral UI state lives in this single
//! cell instead of scattered `Arc<RwLock>`s. Only the main task ever locks it;
//! workers never see this type.

use crate::app::CommandMode;
use crate::view::file::FileViewData;
use crate::view::tree::TreeViewData;
use crate::view::ViewKind;
use std::time::{Duration, Instant};

/// Severity of a transient script notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warn,
    Error,
}

/// State of the keybinding-help overlay.
#[derive(Default, Clone, Debug)]
pub struct HelpState {
    /// Vertical scroll offset, clamped while drawing.
    pub scroll: usize,
}

/// Conflict-resolution state for the merge target, rendered inline.
///
/// Folding is display-only: the file on disk keeps its markers, so a choice can
/// be revisited and (for jj) the conflict can be confirmed as-is.
#[derive(Clone, Debug)]
pub struct ResolveState {
    /// Parsed conflicts from the working file.
    pub conflicts: crate::diff::ConflictedFile,
    /// Chosen side per conflict; `None` keeps the markers.
    pub choices: Vec<Option<crate::diff::Side>>,
    /// Whether each conflict is collapsed to a header/footer frame.
    pub folded: Vec<bool>,
}

impl ResolveState {
    /// Builds a resolver with nothing chosen or folded.
    pub fn new(conflicts: crate::diff::ConflictedFile) -> Self {
        let count = conflicts.conflict_count();
        Self {
            conflicts,
            choices: vec![None; count],
            folded: vec![false; count],
        }
    }

    /// Number of conflicts.
    pub fn count(&self) -> usize {
        self.conflicts.conflict_count()
    }

    /// Number of conflicts that have a chosen side.
    pub fn resolved_count(&self) -> usize {
        self.choices.iter().filter(|c| c.is_some()).count()
    }

    /// The chosen side for conflict `index`.
    pub fn choice(&self, index: usize) -> Option<crate::diff::Side> {
        self.choices.get(index).copied().flatten()
    }

    /// Whether conflict `index` is folded.
    pub fn is_folded(&self, index: usize) -> bool {
        self.folded.get(index).copied().unwrap_or(false)
    }

    /// Sets the chosen side for conflict `index`.
    pub fn set_choice(&mut self, index: usize, side: crate::diff::Side) {
        if let Some(slot) = self.choices.get_mut(index) {
            *slot = Some(side);
        }
    }

    /// Clears the chosen side for conflict `index`, keeping the markers.
    pub fn reset_choice(&mut self, index: usize) {
        if let Some(slot) = self.choices.get_mut(index) {
            *slot = None;
        }
    }

    /// Sets the folded state for conflict `index`.
    pub fn set_folded(&mut self, index: usize, folded: bool) {
        if let Some(slot) = self.folded.get_mut(index) {
            *slot = folded;
        }
    }

    /// The conflict whose (canonical) line range contains `line`.
    ///
    /// The cursor indexes the underlying diff content, which always carries the
    /// full markers — folding only affects rendering.
    pub fn conflict_at_line(&self, line: usize) -> Option<usize> {
        self.conflicts
            .marker_ranges()
            .into_iter()
            .position(|r| r.contains(&line))
    }

    /// On-screen content: folded conflicts show a header framing the chosen
    /// lines plus a footer.
    pub fn display_text(&self) -> String {
        self.conflicts.display(&self.folded, &self.choices)
    }

    /// True when `line` is one of conflict `index`'s kept (chosen-side) lines.
    pub fn keeps_line(&self, index: usize, line: usize) -> bool {
        let start = match self.conflicts.marker_ranges().get(index) {
            Some(r) => r.start,
            None => return false,
        };
        let mut idx = 0;
        for segment in &self.conflicts.segments {
            if let crate::diff::conflict::Segment::Conflict(c) = segment {
                if idx == index {
                    let choice = self.choices.get(index).copied().flatten();
                    return c
                        .kept_ranges(start, choice)
                        .iter()
                        .any(|r| r.contains(&line));
                }
                idx += 1;
            }
        }
        false
    }

    /// Foldable regions for the file-view row model.
    pub fn regions(&self) -> crate::view::file::view_model::ConflictRegions {
        use crate::diff::conflict::Segment;
        use crate::view::file::view_model::ConflictSides;
        let ranges = self.conflicts.marker_ranges();
        // Absolute line ranges of the kept side per conflict, so a folded
        // conflict shows its header plus the chosen lines (editable hunks)
        // while markers, base and the losing side stay hidden.
        let mut kept: Vec<Vec<std::ops::Range<usize>>> = Vec::new();
        let mut sides: Vec<ConflictSides> = Vec::new();
        let mut idx = 0usize;
        for segment in &self.conflicts.segments {
            if let Segment::Conflict(c) = segment {
                let start = ranges.get(idx).map(|r| r.start).unwrap_or(0);
                let (ours, sep, theirs) = c.layout_ranges(start);
                sides.push(ConflictSides { ours, theirs, sep });
                let choice = self.choices.get(idx).copied().flatten();
                kept.push(c.kept_ranges(start, choice));
                idx += 1;
            }
        }
        crate::view::file::view_model::ConflictRegions {
            ranges,
            folded: self.folded.clone(),
            headers: (0..self.count())
                .map(|i| crate::diff::conflict::header_line(self.choice(i)))
                .collect(),
            footers: (0..self.count())
                .map(|_| crate::diff::conflict::footer_line())
                .collect(),
            choices: self.choices.clone(),
            kept,
            sides,
        }
    }

    /// Content to write on confirm: chosen sides applied, others keep markers.
    pub fn resolved_text(&self) -> String {
        self.conflicts.resolve_optional(&self.choices)
    }
}

/// A transient message shown on the bottom bar.
#[derive(Clone, Debug)]
pub struct Message {
    pub level: MessageLevel,
    pub text: String,
    pub expires_at: Instant,
}

impl Message {
    /// Default lifetime of a script notification.
    pub const DEFAULT_TTL: Duration = Duration::from_secs(4);

    /// Builds a message that fades after `ttl`.
    pub fn new(level: MessageLevel, text: String, ttl: Duration) -> Self {
        Self {
            level,
            text: sanitize_panel_text(&text),
            expires_at: Instant::now() + ttl,
        }
    }
}

/// Flattens text to a single line and caps its length for the bottom bar.
pub(crate) fn sanitize_panel_text(text: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut out = String::with_capacity(text.len().min(MAX_CHARS));
    for c in text.chars().take(MAX_CHARS) {
        out.push(if c == '\n' || c == '\r' { ' ' } else { c });
    }
    if text.chars().nth(MAX_CHARS).is_some() {
        out.push('…');
    }
    out
}

/// Selection, scrolling, folds, and command mode of the running application.
pub struct UiState {
    /// Which pane (tree or file) receives input.
    pub current: ViewKind,
    /// Command-line prompt state (`:` mode).
    pub command_mode: CommandMode,
    /// Set when the main loop should exit.
    pub should_quit: bool,
    /// Whether the merge confirmation window is armed.
    pub confirm_merge_window_enabled: bool,
    /// Tree pane: selection, scroll, folds, cached rows.
    pub tree_view: TreeViewData,
    /// File pane: selection, scroll, folds, cached rows.
    pub file_view: FileViewData,
    /// Script-defined hint-bar formats, keyed by view name.
    pub hint_formats: std::collections::HashMap<String, String>,
    /// Transient script notification shown on the bottom bar.
    pub message: Option<Message>,
    /// Persistent script status line shown on the bottom bar.
    pub status: String,
    /// Open keybinding-help overlay, if any.
    pub help: Option<HelpState>,
    /// Conflict-resolution state for the merge target, if any.
    pub resolve: Option<ResolveState>,
    /// Whether the session may write a result (false for `diff --no-write`).
    pub writable: bool,
    /// Bounded undo/redo history for staging mutations.
    pub undo: crate::app::undo::UndoStack,
}

impl UiState {
    /// Builds UI state; `use_gradient` selects gradient line rendering.
    pub fn new(use_gradient: bool) -> Self {
        Self {
            current: ViewKind::Tree,
            command_mode: CommandMode::Normal,
            should_quit: false,
            confirm_merge_window_enabled: false,
            tree_view: TreeViewData::default(),
            file_view: FileViewData::new(use_gradient),
            hint_formats: std::collections::HashMap::new(),
            message: None,
            status: String::new(),
            help: None,
            resolve: None,
            writable: true,
            undo: crate::app::undo::UndoStack::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_state_tracks_choices_and_line_ranges() {
        let source = "\
a
<<<<<<< ours
one
=======
two
>>>>>>> theirs
b
";
        let parsed = crate::diff::ConflictedFile::parse(source).expect("conflicted");
        let mut state = ResolveState::new(parsed);

        assert_eq!(state.count(), 1);
        assert_eq!(state.resolved_count(), 0);
        assert!(state.resolved_text().contains("<<<<<<<"));
        // The unresolved marker block spans lines 1..6 in the rendering.
        assert_eq!(state.conflict_at_line(3), Some(0));

        state.set_choice(0, crate::diff::Side::Ours);
        assert_eq!(state.resolved_count(), 1);
        assert!(state.resolved_text().contains("one"));
        assert!(!state.resolved_text().contains("two"));

        // Folding changes only the rendering: the canonical cursor mapping and
        // the frame text, never the underlying content.
        state.set_folded(0, true);
        assert_eq!(state.conflict_at_line(1), Some(0));
        assert_eq!(state.conflict_at_line(3), Some(0));
        assert!(state.display_text().contains("ours"));
        assert!(!state.display_text().contains("<<<<<<<"));
        assert!(state.keeps_line(0, 2));
        assert!(!state.keeps_line(0, 4));
    }

    #[test]
    fn panel_text_is_flattened_and_capped() {
        assert_eq!(sanitize_panel_text("a\nb\r\nc"), "a b  c");
        let long = "x".repeat(500);
        let out = sanitize_panel_text(&long);
        assert_eq!(out.chars().count(), 241, "240 chars plus ellipsis");
        assert!(out.ends_with('…'));
    }
}
