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
            undo: crate::app::undo::UndoStack::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_text_is_flattened_and_capped() {
        assert_eq!(sanitize_panel_text("a\nb\r\nc"), "a b  c");
        let long = "x".repeat(500);
        let out = sanitize_panel_text(&long);
        assert_eq!(out.chars().count(), 241, "240 chars plus ellipsis");
        assert!(out.ends_with('…'));
    }
}
