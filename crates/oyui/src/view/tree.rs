use crate::ui_state::TreeUiState;
use core_lib::diff_cache::{DiffCache, DiffStats};
use core_lib::lazy;
use core_lib::tree::{FileTree, StagingState, TreeNode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::path::PathBuf;

use super::{
    ViewAction, CLR_ADD_FG, CLR_BG, CLR_CURSOR_BG, CLR_DEL_FG, CLR_DIM, CLR_DIR, CLR_FG,
    CLR_PARTIAL, CLR_STAGED, CLR_UNSTAGED,
};

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
    pub pending_g: bool,
}

impl TreeViewData {
    pub fn flat_rows(&self, tree: &FileTree, cache: &DiffCache) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        let visible_nodes: Vec<&TreeNode> = tree
            .nodes
            .iter()
            .filter(|node| should_show_node(node, cache))
            .collect();

        let count = visible_nodes.len();
        for (i, node) in visible_nodes.into_iter().enumerate() {
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

    pub fn handle_input(
        &mut self,
        key: KeyEvent,
        tree: &FileTree,
        cache: &DiffCache,
    ) -> ViewAction {
        let len = self.flat_rows(tree, cache).len();
        let max_idx = len.saturating_sub(1);
        let mut clear_pending = true;
        let mut action = ViewAction::None;
        let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

        match (key.code, is_ctrl) {
            (KeyCode::Char('c'), true) => action = ViewAction::QuitWithAbort,
            (KeyCode::Char('j'), true) => {
                self.selected_index = (self.selected_index + 5).min(max_idx)
            }
            (KeyCode::Char('k'), true) => {
                self.selected_index = self.selected_index.saturating_sub(5)
            }
            (KeyCode::Char('d'), true) => {
                self.selected_index = (self.selected_index + 20).min(max_idx)
            }
            (KeyCode::Char('u'), true) => {
                self.selected_index = self.selected_index.saturating_sub(20)
            }

            (KeyCode::Char('q'), false) => action = ViewAction::QuitWithAbort,
            (KeyCode::Char('j'), false) | (KeyCode::Down, _) => {
                self.selected_index = (self.selected_index + 1).min(max_idx)
            }
            (KeyCode::Char('k'), false) | (KeyCode::Up, _) => {
                self.selected_index = self.selected_index.saturating_sub(1)
            }

            (KeyCode::Char('G'), false) => self.selected_index = max_idx,
            (KeyCode::Char('g'), false) => {
                if self.pending_g {
                    self.selected_index = 0;
                    self.pending_g = false;
                    clear_pending = false;
                } else {
                    self.pending_g = true;
                    clear_pending = false;
                }
            }

            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => {
                if let Some(row) = self.selected_row(tree, cache) {
                    if row.is_dir {
                        self.ui_state.set_folded(&row.path, false);
                    } else {
                        action = ViewAction::OpenFileView {
                            path: row.path,
                            left_path: row.left_path,
                            right_path: row.right_path,
                        };
                    }
                }
            }
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => {
                if let Some(row) = self.selected_row(tree, cache) {
                    if row.is_dir {
                        self.ui_state.set_folded(&row.path, true);
                    }
                }
            }

            (KeyCode::Enter, _) => action = ViewAction::ConfirmMerge,
            (KeyCode::Char(' '), false) => action = ViewAction::ToggleStageSelected,
            (KeyCode::Char('i'), false) => action = ViewAction::InvertSelection,
            (KeyCode::Char(':'), false) => action = ViewAction::OpenCommandMode,

            _ => {}
        }

        if clear_pending {
            self.pending_g = false;
        }

        action
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
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        self.draw_header(frame, header, cache, base_path, diff_summary);
        self.draw_tree_body(frame, body, tree, cache);
    }

    fn draw_header(
        &self,
        frame: &mut Frame,
        area: Rect,
        cache: &DiffCache,
        base_path: Option<&PathBuf>,
        diff_summary: (usize, usize, usize),
    ) {
        let (a, d, m) = diff_summary;
        let (tot_ins, tot_del) = cache.stats.iter().fold((0, 0), |acc, s| {
            if let lazy::Lazy::Ready(s) = s {
                (acc.0 + s.insertions, acc.1 + s.deletions)
            } else {
                acc
            }
        });

        let path = base_path
            .map(|p| p.to_string_lossy())
            .unwrap_or_else(|| ".".into());

        let left_spans = vec![
            Span::styled(
                format!(" {} ", path),
                Style::default().bg(Color::Rgb(40, 40, 50)).fg(CLR_FG),
            ),
            Span::raw("  "),
            Span::styled(format!("{}A ", a), Style::default().fg(CLR_ADD_FG)),
            Span::styled(format!("{}D ", d), Style::default().fg(CLR_DEL_FG)),
            Span::styled(format!("{}M ", m), Style::default().fg(Color::Yellow)),
        ];

        let right_spans = vec![
            Span::styled(format!("+{} ", tot_ins), Style::default().fg(CLR_ADD_FG)),
            Span::styled(format!("-{} ", tot_del), Style::default().fg(CLR_DEL_FG)),
        ];

        let chunks = Layout::horizontal([Constraint::Min(0), Constraint::Length(20)]).split(area);
        frame.render_widget(Paragraph::new(Line::from(left_spans)).bg(CLR_BG), chunks[0]);
        frame.render_widget(
            Paragraph::new(Line::from(right_spans))
                .alignment(ratatui::layout::Alignment::Right)
                .bg(CLR_BG),
            chunks[1],
        );
    }

    fn draw_tree_body(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        tree: &FileTree,
        cache: &DiffCache,
    ) {
        let rows = self.flat_rows(tree, cache);
        let items: Vec<ListItem> = rows.iter().map(render_tree_row).collect();
        let mut list_state = ListState::default();
        list_state.select(Some(self.selected_index));

        let list = List::new(items)
            .block(Block::default().style(Style::default().bg(CLR_BG)))
            .highlight_style(Style::default().bg(CLR_CURSOR_BG));

        frame.render_stateful_widget(list, area, &mut list_state);
    }
}

fn get_diff_color(value: usize, is_addition: bool) -> Color {
    // Normalize to 0.0 - 1.0
    let t = (value as f64 / 100.0).min(1.0);

    // Square root curve: steep at the beginning, flattens out later.
    let factor = t.sqrt();

    // We use a base value (min color) and a range (how much to add).
    let min_val = 120;
    let range = 125.0;

    if is_addition {
        // Start: Rgb(60, 120, 60) -> End: Rgb(60, 255, 60)
        let g = (min_val as f64 + (range * factor)) as u8;
        Color::Rgb(60, g, 60)
    } else {
        // Start: Rgb(120, 60, 60) -> End: Rgb(255, 60, 60)
        let r = (min_val as f64 + (range * factor)) as u8;
        Color::Rgb(r, 60, 60)
    }
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
            if let Some(s) = &stats {
                if s.insertions == 0 && s.deletions == 0 {
                    return;
                }
            }

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
            let folded = ui_state.is_folded(&dir.path);
            let staging_state = node.compute_staging_state();

            rows.push(TreeRow {
                path: dir.path.clone(),
                name: dir.name.clone(),
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
                let visible_children: Vec<&TreeNode> = dir
                    .children
                    .iter()
                    .filter(|child| should_show_node(child, cache))
                    .collect();
                let mut child_continuations = parent_continuations.to_vec();
                child_continuations.push(!is_last);
                let child_count = visible_children.len();
                for (i, child) in visible_children.into_iter().enumerate() {
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

fn should_show_node(node: &TreeNode, cache: &DiffCache) -> bool {
    match node {
        TreeNode::File(f) => {
            let stats_lazy = cache.stats.get(&f.path);
            if let Some(s) = stats_lazy.value() {
                s.insertions > 0 || s.deletions > 0
            } else {
                true
            }
        }
        TreeNode::Directory(d) => d
            .children
            .iter()
            .any(|child| should_show_node(child, cache)),
    }
}

fn render_tree_row(row: &TreeRow) -> ListItem<'_> {
    let mut spans = Vec::new();

    // 1. Determine the base color for the entire row based on status
    let base_fg = if !row.is_dir {
        if row.left_path.is_none() {
            CLR_ADD_FG
        } else if row.right_path.is_none() {
            CLR_DEL_FG
        } else {
            CLR_FG
        }
    } else {
        CLR_FG
    };

    // 2. Tree structure spans (keep these structural)
    for &has_sibling in &row.parent_continuations {
        spans.push(Span::styled(
            if has_sibling { "│  " } else { "   " },
            Style::default().fg(CLR_DIM),
        ));
    }
    spans.push(Span::styled(
        if row.is_last {
            "└── "
        } else {
            "├── "
        },
        Style::default().fg(CLR_DIM),
    ));

    // 3. Staging symbols
    let (stage_sym, stage_color) = match row.staging_state {
        StagingState::Staged => ("●", CLR_STAGED),
        StagingState::Unstaged => ("○", CLR_UNSTAGED),
        StagingState::PartiallyStaged => ("◐", CLR_PARTIAL),
    };
    spans.push(Span::styled(stage_sym, Style::default().fg(stage_color)));
    spans.push(Span::raw(" "));

    // 4. File/Dir Name and Icon
    if row.is_dir {
        let arrow = if row.is_folded { "▸ " } else { "▾ " };
        spans.push(Span::styled(arrow, Style::default().fg(CLR_FG)));
        spans.push(Span::styled(" ", Style::default().fg(CLR_DIR)));
        spans.push(Span::styled(
            row.name.as_str(),
            Style::default().fg(CLR_DIR).bold(),
        ));
    } else {
        let icon = get_file_icon(&row.name);
        // Using base_fg for the icon and filename to color the whole row
        spans.push(Span::styled(icon.0, Style::default().fg(base_fg)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            row.name.as_str(),
            Style::default().fg(base_fg),
        ));
    }

    // 5. Dynamic Stats
    if let Some(s) = &row.stats {
        if s.insertions > 0 || s.deletions > 0 {
            spans.push(Span::raw("  "));
        }

        if s.insertions > 0 {
            spans.push(Span::styled(
                format!("+{} ", s.insertions),
                Style::default().fg(get_diff_color(s.insertions, true)),
            ));
        }

        if s.deletions > 0 {
            spans.push(Span::styled(
                format!("-{} ", s.deletions),
                Style::default().fg(get_diff_color(s.deletions, false)),
            ));
        }
    }

    ListItem::new(Line::from(spans))
}

fn get_file_icon(name: &str) -> (&'static str, Color) {
    let ext = name.split('.').next_back().unwrap_or("");
    match ext {
        "tsx" | "jsx" => ("", Color::Rgb(80, 200, 255)),
        "ts" | "js" => ("", Color::Rgb(240, 220, 80)),
        "svg" => ("󰕙", Color::Rgb(255, 180, 100)),
        "md" => ("", Color::Rgb(200, 200, 200)),
        "json" => ("", Color::Rgb(250, 200, 50)),
        _ => ("󰈚", Color::Rgb(180, 180, 190)),
    }
}
