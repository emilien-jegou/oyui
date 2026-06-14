use crate::commons::file_icon::FileIconProvider;
use crate::commons::lazy;
use crate::config::UiTheme;
use crate::diff_cache::DiffCache;
use crate::ui_state::TreeUiState;
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

#[derive(Default)]
pub struct TreeViewData {
    pub selected_index: usize,
    pub ui_state: TreeUiState,
    pub scrolloff: usize,
    pub list_state: ListState,
    pub last_height: usize,
}

impl TreeViewData {
    pub fn flat_rows(&self, tree: &FileTree, cache: &DiffCache) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let count = tree.nodes.len();
        for (i, node) in tree.nodes.iter().enumerate() {
            let is_last = i == count - 1;
            flatten_recursive(
                node,
                0,
                is_last,
                &Vec::new(),
                &self.ui_state,
                cache,
                &mut rows,
            );
        }
        rows
    }

    pub fn selected_row(&self, tree: &FileTree, cache: &DiffCache) -> Option<TreeRow> {
        self.flat_rows(tree, cache)
            .into_iter()
            .nth(self.selected_index)
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
    ) {
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        self.draw_header(frame, header, cache, base_path, diff_summary, theme);
        self.draw_tree_body(icon_provider, frame, body, tree, cache, theme);
    }

    fn draw_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
        theme: &UiTheme,
    ) {
        let (a, d, m) = diff_summary;

        let mut tot_ins = 0;
        let mut tot_del = 0;

        let _ = cache.stats.inner().iter_sync(|_, v| {
            if let lazy::Lazy::Ready(DiffStats::Text {
                insertions,
                deletions,
            }) = v
            {
                tot_ins += insertions;
                tot_del += deletions;
            }

            true
        });

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
                format!("+{} ", tot_ins),
                Style::default().fg(theme.add_fg.into()),
            ),
            Span::styled(
                format!("-{} ", tot_del),
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
    ) {
        let rows = self.flat_rows(tree, cache);
        let items: Vec<ListItem> = rows
            .iter()
            .map(|r| render_tree_row(icon_provider, r, theme))
            .collect();
        self.list_state.select(Some(self.selected_index));

        let height = area.height as usize;
        self.last_height = height;
        if height > 0 {
            let selected = self.selected_index;
            let mut offset = self.list_state.offset();
            // Prevent scrolloff from overlapping itself if the screen is tiny
            let scrolloff = self.scrolloff.min(height.saturating_sub(1) / 2);

            if selected < offset + scrolloff {
                offset = selected.saturating_sub(scrolloff);
            } else if selected + scrolloff >= offset + height {
                offset = (selected + scrolloff + 1).saturating_sub(height);
            }
            *self.list_state.offset_mut() = offset;
        }

        let list = List::new(items)
            .block(Block::default().style(Style::default().bg(theme.bg.into())))
            .highlight_style(Style::default().bg(theme.cursor_bg.into()));

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }
}

fn lerp_color(c1: Color, c2: Color, t: f32) -> Color {
    let (r1, g1, b1) = match c1 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };
    let (r2, g2, b2) = match c2 {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (255, 255, 255),
    };
    Color::Rgb(
        (r1 as f32 + (r2 as f32 - r1 as f32) * t).clamp(0.0, 255.0) as u8,
        (g1 as f32 + (g2 as f32 - g1 as f32) * t).clamp(0.0, 255.0) as u8,
        (b1 as f32 + (b2 as f32 - b1 as f32) * t).clamp(0.0, 255.0) as u8,
    )
}

fn get_diff_color(value: usize, is_addition: bool, theme: &UiTheme) -> Color {
    let t = (value as f64 / 100.0).min(1.0).sqrt() as f32;
    let target = if is_addition {
        theme.add_fg.into()
    } else {
        theme.del_fg.into()
    };
    lerp_color(theme.dim.into(), target, t)
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
            let stats = cache.stats.get(&file.path).value().cloned();

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

fn render_tree_row<'a>(
    icon_provider: &dyn FileIconProvider,
    row: &'a TreeRow,
    theme: &'a UiTheme,
) -> ListItem<'a> {
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
            row.name.as_str(),
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
        spans.push(Span::styled(
            row.name.as_str(),
            Style::default().fg(base_fg),
        ));
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
                        Style::default().fg(get_diff_color(*insertions, true, theme)),
                    ));
                }
                if *deletions > 0 {
                    spans.push(Span::styled(
                        format!("-{} ", deletions),
                        Style::default().fg(get_diff_color(*deletions, false, theme)),
                    ));
                }
            }
        }
    }

    ListItem::new(Line::from(spans))
}
