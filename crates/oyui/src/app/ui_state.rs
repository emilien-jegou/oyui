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
        }
    }
}
