pub mod config_error;
pub mod file;
pub mod tree;

use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::PathBuf;

#[derive(Default, PartialEq, Eq)]
pub enum ViewKind {
    #[default]
    Tree,
    File,
}

#[derive(Clone, Debug)]
pub enum ViewAction {
    None,
    QuitWithAbort,
    OpenCommandMode,
    ConfirmMerge,
    ToggleStageSelected,
    ToggleStageHunk(usize),
    InvertSelection,
    OpenFileView {
        path: PathBuf,
        left_path: Option<PathBuf>,
        right_path: Option<PathBuf>,
    },
    CloseFileView,
}

#[derive(Default)]
pub struct View {
    pub current: ViewKind,
    pub tree_view: tree::TreeViewData,
    pub file_view: file::FileViewData,
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    #[tracing::instrument(skip_all)]
    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        tree: &FileTree,
        cache: &DiffCache,
    ) -> ViewAction {
        match self.current {
            ViewKind::Tree => self.tree_view.handle_input(key, tree, cache),
            ViewKind::File => self.file_view.handle_input(key, cache),
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        tree: &FileTree,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
    ) {
        match self.current {
            ViewKind::Tree => {
                self.tree_view
                    .draw(frame, area, tree, cache, base_path, diff_summary, theme)
            }
            ViewKind::File => self.file_view.draw(frame, area, cache, tree, theme),
        }
    }
}
