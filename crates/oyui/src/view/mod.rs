pub mod config_error;
pub mod file;
pub mod tree;
pub mod confirm_window;

use crate::commons::file_icon::DevIconProvider;
use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::tree::FileTree;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum ViewKind {
    #[default]
    Tree,
    File,
}

#[derive(Clone)]
pub struct View {
    pub current: Arc<RwLock<ViewKind>>,
    pub tree_view: Arc<RwLock<tree::TreeViewData>>,
    pub file_view: Arc<RwLock<file::FileViewData>>,
}

impl View {
    pub fn configure(&self, scrolloff: usize, context_lines: usize) {
        self.file_view.write().scrolloff = scrolloff;
        self.file_view.write().context_lines = context_lines;
        self.tree_view.write().scrolloff = scrolloff;
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
        tree: &FileTree,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
        color_mode: &TerminalColorMode
    ) {
        let current = *self.current.read();
        match current {
            ViewKind::Tree => self.tree_view.write().draw(
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
            ViewKind::File => self.file_view.write().draw(frame, area, cache, tree, theme),
        }
    }
}
