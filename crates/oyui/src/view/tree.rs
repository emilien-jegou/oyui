//! Tree view rendering backed by a cached flattened row view model.
use crate::commons::file_icon::FileIconProvider;

use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::terminal_colors::TerminalColorMode;
use crate::ui_state::TreeUiState;
use crate::view::file::utils::colors::try_lerp_color;
use crate::{
    diff::DiffStats,
    tree::{FileTree, StagingState, TreeNode},
};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TreeRow {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_folded: bool,
    pub is_last: bool,
    pub parent_continuations: Vec<bool>,
    pub staging_state: StagingState,
    pub stats: Option<DiffStats>,
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
}

/// Pre-computed flat rows for tree rendering.
#[derive(Default)]
pub struct TreeViewModel {
    rows: Vec<TreeRow>,
    total_ins: usize,
    total_del: usize,
}

impl TreeViewModel {
    /// Recomputes the flat rows for the tree.
    pub fn recompute(&mut self, tree: &FileTree, cache: &DiffCache, ui_state: &TreeUiState) {
        let mut rows = Vec::new();
        let count = tree.nodes.len();
        for (i, node) in tree.nodes.iter().enumerate() {
            let is_last = i == count - 1;
            flatten_recursive(node, 0, is_last, &Vec::new(), ui_state, cache, &mut rows);
        }

        let mut total_ins = 0;
        let mut total_del = 0;
        for row in &rows {
            if let Some(DiffStats::Text {
                insertions,
                deletions,
            }) = &row.stats
            {
                total_ins += *insertions;
                total_del += *deletions;
            }
        }

        self.rows = rows;
        self.total_ins = total_ins;
        self.total_del = total_del;
    }

    /// Returns the flat rows.
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    /// Returns the row at the given index.
    pub fn row(&self, idx: usize) -> Option<&TreeRow> {
        self.rows.get(idx)
    }

    /// Returns the total insertions across all files.
    pub fn total_insertions(&self) -> usize {
        self.total_ins
    }

    /// Returns the total deletions across all files.
    pub fn total_deletions(&self) -> usize {
        self.total_del
    }
}

#[derive(Default)]
pub struct TreeViewData {
    pub selected_index: usize,
    pub ui_state: TreeUiState,
    pub scrolloff: usize,
    pub list_state: ListState,
    pub last_height: usize,
    view_model: TreeViewModel,
    view_model_dirty: bool,
    cached_tree_version: u64,
    cached_stats_version: u64,
}

impl TreeViewData {
    /// Marks the view model as needing recomputation.
    pub fn mark_dirty(&mut self) {
        self.view_model_dirty = true;
    }

    /// Recomputes rows when the fold state, tree, or async stats changed.
    fn ensure_fresh(&mut self, tree: &FileTree, cache: &DiffCache) {
        let stats_version = cache.stats.version();
        if self.view_model_dirty
            || self.cached_tree_version != tree.version()
            || self.cached_stats_version != stats_version
        {
            self.view_model.recompute(tree, cache, &self.ui_state);
            self.cached_tree_version = tree.version();
            self.cached_stats_version = stats_version;
            self.view_model_dirty = false;
        }
    }

    /// Returns the flat rows, recomputing if needed.
    pub fn flat_rows(&mut self, tree: &FileTree, cache: &DiffCache) -> &[TreeRow] {
        self.ensure_fresh(tree, cache);
        self.view_model.rows()
    }

    /// Returns the selected row, recomputing if needed.
    pub fn selected_row(&mut self, tree: &FileTree, cache: &DiffCache) -> Option<TreeRow> {
        self.ensure_fresh(tree, cache);
        self.view_model.row(self.selected_index).cloned()
    }

    /// Returns the total insertions across all files.
    pub fn total_insertions(&self) -> usize {
        self.view_model.total_insertions()
    }

    /// Returns the total deletions across all files.
    pub fn total_deletions(&self) -> usize {
        self.view_model.total_deletions()
    }

    #[tracing::instrument(skip_all)]
    pub fn draw(
        &mut self,
        icon_provider: &dyn FileIconProvider,
        frame: &mut Frame,
        area: Rect,
        tree: &FileTree,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
        color_mode: &TerminalColorMode,
    ) {
        // Refresh before the header reads totals so tree changes show in-frame.
        self.ensure_fresh(tree, cache);
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        self.draw_header(frame, header, base_path, diff_summary, theme);
        self.draw_tree_body(icon_provider, frame, body, tree, cache, theme, color_mode);
    }

    fn draw_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
    ) {
        let (a, d, m) = diff_summary;

        let path = base_path
            .map(|p| p.to_string_lossy())
            .unwrap_or_else(|| ".".into());

        let left_spans = vec![
            Span::styled(
                format!(" {} ", path),
                Style::default()
                    .bg(theme.cursor_bg.into())
                    .fg(theme.fg.into()),
            ),
            Span::raw("  "),
            Span::styled(format!("{}A ", a), Style::default().fg(theme.add_fg.into())),
            Span::styled(format!("{}D ", d), Style::default().fg(theme.del_fg.into())),
            Span::styled(
                format!("{}M ", m),
                Style::default().fg(theme.partial.into()),
            ),
        ];

        let right_spans = vec![
            Span::styled(
                format!("+{} ", self.view_model.total_insertions()),
                Style::default().fg(theme.add_fg.into()),
            ),
            Span::styled(
                format!("-{} ", self.view_model.total_deletions()),
                Style::default().fg(theme.del_fg.into()),
            ),
        ];

        let chunks = Layout::horizontal([Constraint::Min(0), Constraint::Length(20)]).split(area);

        frame.render_widget(
            Paragraph::new(Line::from(left_spans)).bg(theme.bg),
            chunks[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(right_spans))
                .alignment(ratatui::layout::Alignment::Right)
                .bg(theme.bg),
            chunks[1],
        );
    }

    fn draw_tree_body(
        &mut self,
        icon_provider: &dyn FileIconProvider,
        frame: &mut Frame,
        area: Rect,
        tree: &FileTree,
        cache: &DiffCache,
        theme: &UiTheme,
        color_mode: &TerminalColorMode,
    ) {
        let height = area.height as usize;
        let selected = self.selected_index;
        self.last_height = height;

        // Absolute scrolloff anchor, persisted in `list_state.offset`.
        let mut offset = self.list_state.offset();
        if height > 0 {
            // Prevent scrolloff from overlapping itself if the screen is tiny
            let scrolloff = self.scrolloff.min(height.saturating_sub(1) / 2);

            if selected < offset + scrolloff {
                offset = selected.saturating_sub(scrolloff);
            } else if selected + scrolloff >= offset + height {
                offset = (selected + scrolloff + 1).saturating_sub(height);
            }
        }

        // Build items only for the visible window; owned spans keep them
        // 'static so the view-model borrow ends before `list_state` is reused.
        let (items, start, end) = {
            let rows = self.flat_rows(tree, cache);
            let start = offset.min(rows.len().saturating_sub(1));
            let end = (start + height).min(rows.len());
            let items = rows[start..end]
                .iter()
                .map(|r| render_tree_row(icon_provider, r, theme, color_mode))
                .collect::<Vec<ListItem>>();
            (items, start, end)
        };

        self.list_state.select(Some(selected));
        if height > 0 {
            *self.list_state.offset_mut() = offset;
        }

        // Window-relative state: the stored anchor stays absolute while
        // ratatui only ever sees rows inside the slice.
        let mut rel_state = ListState::default();
        if end > start {
            rel_state.select(Some(selected.clamp(start, end - 1) - start));
        }

        let list = List::new(items)
            .block(Block::default().style(Style::default().bg(theme.bg.into())))
            .highlight_style(Style::default().bg(theme.cursor_bg.into()));

        frame.render_stateful_widget(list, area, &mut rel_state);
    }
}

fn get_diff_color(
    value: usize,
    is_addition: bool,
    theme: &UiTheme,
    color_mode: &TerminalColorMode,
) -> Color {
    let target = if is_addition {
        theme.add_fg
    } else {
        theme.del_fg
    };

    if color_mode.support_true_color() && theme.tree_progressive_change_dim {
        let t = (value as f32 / 100.0).min(1.0).sqrt();
        if let Some(color) = try_lerp_color(&theme.dim, &target, t) {
            return color.into();
        }
    }

    target.into()
}

fn flatten_recursive(
    node: &TreeNode,
    depth: usize,
    is_last: bool,
    parent_continuations: &[bool],
    ui_state: &TreeUiState,
    cache: &DiffCache,
    rows: &mut Vec<TreeRow>,
) {
    match node {
        TreeNode::File(file) => {
            let stats = cache.stats.get(&file.path).as_deref().cloned();

            rows.push(TreeRow {
                path: file.path.clone(),
                name: file.name.clone(),
                depth,
                is_dir: false,
                is_folded: false,
                is_last,
                parent_continuations: parent_continuations.to_vec(),
                staging_state: file.state,
                stats,
                left_path: file.left_path.clone(),
                right_path: file.right_path.clone(),
            });
        }
        TreeNode::Directory(dir) => {
            let mut current_dir = dir;
            let mut combined_name = current_dir.name.clone();

            // Look ahead: if the directory only contains exactly 1 directory child, compress it!
            while current_dir.children.len() == 1 {
                if let TreeNode::Directory(child_dir) = &current_dir.children[0] {
                    combined_name.push('/');
                    combined_name.push_str(&child_dir.name);
                    current_dir = child_dir;
                } else {
                    break;
                }
            }

            let folded = ui_state.is_folded(&current_dir.path);
            let staging_state = node.compute_staging_state();

            rows.push(TreeRow {
                path: current_dir.path.clone(),
                name: combined_name,
                depth,
                is_dir: true,
                is_folded: folded,
                is_last,
                parent_continuations: parent_continuations.to_vec(),
                staging_state,
                stats: None,
                left_path: None,
                right_path: None,
            });

            if !folded {
                let mut child_continuations = parent_continuations.to_vec();
                child_continuations.push(!is_last);
                let child_count = current_dir.children.len();
                for (i, child) in current_dir.children.iter().enumerate() {
                    let child_is_last = i == child_count - 1;
                    flatten_recursive(
                        child,
                        depth + 1,
                        child_is_last,
                        &child_continuations,
                        ui_state,
                        cache,
                        rows,
                    );
                }
            }
        }
    }
}

fn render_tree_row(
    icon_provider: &dyn FileIconProvider,
    row: &TreeRow,
    theme: &UiTheme,
    color_mode: &TerminalColorMode,
) -> ListItem<'static> {
    let mut spans = Vec::new();

    // 1. Determine the base color for the entire row based on status
    let base_fg: Color = if !row.is_dir {
        if row.left_path.is_none() {
            theme.add_fg.into()
        } else if row.right_path.is_none() {
            theme.del_fg.into()
        } else {
            theme.fg.into()
        }
    } else {
        theme.fg.into()
    };

    // 2. Tree structure spans (keep these structural)
    for &has_sibling in &row.parent_continuations {
        spans.push(Span::styled(
            if has_sibling { "│  " } else { "   " },
            Style::default().fg(theme.dim.into()),
        ));
    }
    spans.push(Span::styled(
        if row.is_last {
            "└── "
        } else {
            "├── "
        },
        Style::default().fg(theme.dim.into()),
    ));

    // 3. Staging symbols
    let (stage_sym, stage_color): (&str, Color) = match row.staging_state {
        StagingState::Staged => ("●", theme.staged.into()),
        StagingState::Unstaged => ("○", theme.unstaged.into()),
        StagingState::PartiallyStaged => ("◐", theme.partial.into()),
    };
    spans.push(Span::styled(stage_sym, Style::default().fg(stage_color)));
    spans.push(Span::raw(" "));

    // 4. File/Dir Name and Icon
    let left_name = row
        .left_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy());
    let right_name = row
        .right_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy());

    let name_differ = match (&left_name, &right_name) {
        (Some(l), Some(r)) => l != r,
        _ => false,
    };

    if row.is_dir {
        let arrow = if row.is_folded { "▸ " } else { "▾ " };
        spans.push(Span::styled(arrow, Style::default().fg(theme.fg.into())));
        spans.push(Span::styled(" ", Style::default().fg(theme.dir.into())));
        spans.push(Span::styled(
            row.name.clone(),
            Style::default().fg(theme.dir.into()).bold(),
        ));
    } else if name_differ {
        let l_name = left_name.as_ref().unwrap().to_string();
        let r_name = right_name.as_ref().unwrap().to_string();

        spans.push(Span::styled("[ ", Style::default().fg(theme.dim.into())));
        spans.push(Span::styled(l_name, Style::default().fg(base_fg)));
        spans.push(Span::styled(" → ", Style::default().fg(theme.dim.into())));
        spans.push(Span::styled(r_name, Style::default().fg(base_fg)));
        spans.push(Span::styled(" ]", Style::default().fg(theme.dim.into())));
    } else {
        let icon = icon_provider.get_file_icon(&row.name);

        spans.push(Span::styled(icon.to_string(), Style::default().fg(base_fg)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(row.name.clone(), Style::default().fg(base_fg)));
    }

    // 5. Dynamic Stats
    if let Some(stats) = &row.stats {
        match stats {
            DiffStats::Binary { bytes } => {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    "(binary)",
                    Style::default().fg(theme.dir.into()),
                ));
                spans.push(Span::raw(" "));
                let sign = if *bytes > 0 { "+" } else { "" };
                spans.push(Span::styled(
                    format!("{}{} bytes ", sign, bytes),
                    Style::default().fg(theme.dim.into()),
                ));
            }
            DiffStats::Text {
                insertions,
                deletions,
            } => {
                if *insertions > 0 || *deletions > 0 {
                    spans.push(Span::raw("  "));
                }
                if *insertions > 0 {
                    spans.push(Span::styled(
                        format!("+{} ", insertions),
                        Style::default().fg(get_diff_color(*insertions, true, theme, color_mode)),
                    ));
                }
                if *deletions > 0 {
                    spans.push(Span::styled(
                        format!("-{} ", deletions),
                        Style::default().fg(get_diff_color(*deletions, false, theme, color_mode)),
                    ));
                }
            }
        }
    }

    ListItem::new(Line::from(spans))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::StagingState;
    use std::sync::Arc;

    fn tree_with_file() -> FileTree {
        let mut tree = FileTree::default();
        tree.insert_file(
            PathBuf::from("a.txt"),
            None,
            Some(PathBuf::from("right/a.txt")),
        );
        tree
    }

    /// The worker populates the tree after the first frame drew an empty view;
    /// the cached view model must pick the files up without a manual dirty mark.
    #[test]
    fn flat_rows_refresh_after_worker_populates_tree() {
        let mut view = TreeViewData::default();
        let cache = DiffCache::default();

        assert!(view.flat_rows(&FileTree::default(), &cache).is_empty());

        let populated = tree_with_file();
        assert_eq!(
            view.flat_rows(&populated, &cache).len(),
            1,
            "tree view must refresh after the worker populates the tree"
        );
    }

    /// Stats are computed asynchronously after the first frame; the cached
    /// rows must pick them up without a tree mutation.
    #[test]
    fn flat_rows_refresh_after_stats_arrive() {
        let mut view = TreeViewData::default();
        let cache = DiffCache::default();
        let tree = tree_with_file();

        assert!(view.flat_rows(&tree, &cache)[0].stats.is_none());

        let stats = DiffStats::Text {
            insertions: 12,
            deletions: 16,
        };
        cache.stats.set(
            PathBuf::from("a.txt"),
            Arc::new(stats.clone()),
            cache.stats.generation(),
        );

        assert_eq!(
            view.flat_rows(&tree, &cache)[0].stats,
            Some(stats),
            "stats written after the first draw must invalidate the cached rows"
        );
    }

    /// Staging toggles mutate the tree behind the view's back; cached rows
    /// carry staging state and must be recomputed from the new tree state.
    #[test]
    fn flat_rows_refresh_after_staging_state_change() {
        let mut view = TreeViewData::default();
        let cache = DiffCache::default();
        let mut tree = tree_with_file();
        assert_eq!(view.flat_rows(&tree, &cache).len(), 1);

        tree.set_state_for_path(&PathBuf::from("a.txt"), StagingState::Staged);

        let rows = view.flat_rows(&tree, &cache);
        assert_eq!(
            rows[0].staging_state,
            StagingState::Staged,
            "staging toggle must invalidate the cached rows"
        );
    }
}
