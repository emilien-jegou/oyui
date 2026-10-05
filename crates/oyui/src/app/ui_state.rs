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

/// Interactive conflict-resolution state for the merge target.
#[derive(Clone, Debug)]
pub struct ResolveState {
    /// Parsed conflicts from the target file.
    pub conflicts: crate::diff::ConflictedFile,
    /// Index of the conflict being inspected.
    pub cursor: usize,
    /// Chosen side per conflict; `None` keeps the markers.
    pub choices: Vec<Option<crate::diff::Side>>,
}

impl ResolveState {
    /// Builds a resolver with nothing chosen yet.
    pub fn new(conflicts: crate::diff::ConflictedFile) -> Self {
        let choices = vec![None; conflicts.conflict_count()];
        Self {
            conflicts,
            cursor: 0,
            choices,
        }
    }

    /// Number of conflicts.
    pub fn count(&self) -> usize {
        self.conflicts.conflict_count()
    }

    /// The conflict currently under the cursor.
    pub fn current(&self) -> Option<&crate::diff::conflict::Conflict> {
        self.conflicts
            .segments
            .iter()
            .filter_map(|s| match s {
                crate::diff::conflict::Segment::Conflict(c) => Some(c),
                _ => None,
            })
            .nth(self.cursor)
    }

    /// Moves the cursor by `delta`, clamped to the conflict range.
    pub fn move_cursor(&mut self, delta: isize) {
        let count = self.count();
        if count == 0 {
            self.cursor = 0;
            return;
        }
        self.cursor = (self.cursor as isize + delta).clamp(0, count as isize - 1) as usize;
    }

    /// Chooses a side for the current conflict.
    pub fn set_choice(&mut self, side: crate::diff::Side) {
        if let Some(slot) = self.choices.get_mut(self.cursor) {
            *slot = Some(side);
        }
    }

    /// Clears the current conflict's choice.
    pub fn clear_choice(&mut self) {
        if let Some(slot) = self.choices.get_mut(self.cursor) {
            *slot = None;
        }
    }

    /// Number of conflicts that have a chosen side.
    pub fn resolved_count(&self) -> usize {
        self.choices.iter().filter(|c| c.is_some()).count()
    }

    /// Renders the file with the chosen sides; unresolved conflicts keep markers.
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
    /// Whether the conflict resolver overlay is visible.
    pub resolve_open: bool,
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
            resolve_open: false,
            writable: true,
            undo: crate::app::undo::UndoStack::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_state_navigates_and_chooses_sides() {
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

        state.set_choice(crate::diff::Side::Ours);
        assert_eq!(state.resolved_count(), 1);
        assert!(state.resolved_text().contains("one"));
        assert!(!state.resolved_text().contains("two"));

        state.clear_choice();
        assert!(state.resolved_text().contains("<<<<<<<"));
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
