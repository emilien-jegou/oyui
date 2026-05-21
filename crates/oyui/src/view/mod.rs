pub mod file;
pub mod tree;

use crate::diff_cache::DiffCache;
use crate::tree::FileTree;
use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;
use std::path::PathBuf;

pub const CLR_BG: Color = Color::Rgb(14, 14, 18);
pub const CLR_CURSOR_BG: Color = Color::Rgb(30, 30, 42);
pub const CLR_FG: Color = Color::Rgb(200, 200, 210);
pub const CLR_DIM: Color = Color::Rgb(70, 70, 85);
pub const CLR_STAGED: Color = Color::Rgb(130, 210, 150);
pub const CLR_UNSTAGED: Color = Color::Rgb(110, 110, 110);
pub const CLR_PARTIAL: Color = Color::Rgb(210, 170, 80);
pub const CLR_DIR: Color = Color::Rgb(100, 140, 210);
pub const CLR_CMD: Color = Color::Rgb(180, 140, 255);
pub const CLR_ADD_BG: Color = Color::Rgb(30, 45, 30);
pub const CLR_DEL_BG: Color = Color::Rgb(45, 30, 30);
pub const CLR_ADD_FG: Color = Color::Rgb(130, 255, 130);
pub const CLR_DEL_FG: Color = Color::Rgb(255, 130, 130);

#[derive(Default, PartialEq, Eq)]
pub enum ViewKind {
    #[default]
    Tree,
    File,
}

pub enum ViewAction {
    None,
    QuitWithAbort,
    OpenCommandMode,
    ConfirmMerge,
    ToggleStageSelected,
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

    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        tree: &FileTree,
        cache: &DiffCache,
    ) -> ViewAction {
        match self.current {
            ViewKind::Tree => self.tree_view.handle_input(key, tree, cache),
            ViewKind::File => self.file_view.handle_input(key),
        }
    }

    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        tree: &FileTree,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
    ) {
        match self.current {
            ViewKind::Tree => {
                self.tree_view
                    .draw(frame, area, tree, cache, base_path, diff_summary)
            }
            ViewKind::File => self.file_view.draw(frame, area, cache),
        }
    }
}
