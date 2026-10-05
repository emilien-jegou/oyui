//! Rendering orchestration across the tree and file panes.

pub mod config_error;
pub mod confirm_window;
pub mod file;
pub mod keybinds;
pub mod tree;

use crate::app::UiState;
use crate::commons::file_icon::DevIconProvider;
use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::tree::FileTree;
use std::path::PathBuf;

#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ViewKind {
    #[default]
    Tree,
    File,
}

impl UiState {
    /// Applies scroll settings to both panes.
    pub fn configure(&mut self, scrolloff: usize, context_lines: usize) {
        self.file_view.scrolloff = scrolloff;
        self.file_view.context_lines = context_lines;
        self.tree_view.scrolloff = scrolloff;
    }

    /// Draws the active pane; all UI state is read through the one lock.
    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        tree: &FileTree,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
        color_mode: &TerminalColorMode,
    ) {
        match self.current {
            ViewKind::Tree => self.tree_view.draw(
                &DevIconProvider,
                frame,
                area,
                tree,
                cache,
                base_path,
                diff_summary,
                theme,
                color_mode,
            ),
            ViewKind::File => self.file_view.draw(frame, area, cache, tree, theme),
        }
    }
}
