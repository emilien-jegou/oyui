pub mod mapping;
pub mod render;
pub mod utils;

use ratatui::widgets::TableState;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct FileViewData {
    pub scroll_states: HashMap<PathBuf, TableState>,
    pub hscroll_states: HashMap<PathBuf, usize>,
    pub row_counts: HashMap<PathBuf, usize>,
    pub line_mapping: HashMap<PathBuf, Vec<usize>>,
    pub hunk_starts: HashMap<PathBuf, Vec<usize>>,
    pub row_to_hunk: HashMap<PathBuf, Vec<Option<usize>>>,
    pub current_path: Option<PathBuf>,
    pub pending_g: bool,
    pub scrolloff: usize,
    pub is_folded: bool,
    pub context_lines: usize,
    pub last_height: usize,
    pub last_width: usize,
    pub use_gradient: bool,
}

impl Default for FileViewData {
    fn default() -> Self {
        Self {
            scroll_states: HashMap::new(),
            hscroll_states: HashMap::new(),
            row_counts: HashMap::new(),
            line_mapping: HashMap::new(),
            hunk_starts: HashMap::new(),
            row_to_hunk: HashMap::new(),
            current_path: None,
            pending_g: false,
            scrolloff: 0,
            is_folded: true,
            context_lines: 4,
            last_height: 0,
            last_width: 0,
            use_gradient: true,
        }
    }
}
