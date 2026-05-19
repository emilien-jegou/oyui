use std::collections::HashMap;
use std::path::PathBuf;

/// Per-node TUI display state. Keyed by path, lives entirely in the TUI layer.
/// The core tree knows nothing about this.
#[derive(Debug, Clone, Default)]
pub struct NodeUiState {
    pub is_folded: bool,
}

#[derive(Debug, Default)]
pub struct TreeUiState {
    nodes: HashMap<PathBuf, NodeUiState>,
}

impl TreeUiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_folded(&self, path: &PathBuf) -> bool {
        self.nodes.get(path).is_some_and(|s| s.is_folded)
    }

    pub fn toggle_folded(&mut self, path: &PathBuf) {
        let state = self.nodes.entry(path.clone()).or_default();
        state.is_folded = !state.is_folded;
    }

    pub fn set_folded(&mut self, path: &PathBuf, value: bool) {
        self.nodes.entry(path.clone()).or_default().is_folded = value;
    }
}
