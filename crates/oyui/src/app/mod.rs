pub mod commands;
pub mod draw;
pub mod events;
pub mod merge;

pub use events::{CommandMode, ExitAction};
use typed_builder::TypedBuilder;

use crate::actions::state::TuiState;
use crate::actions::BoxedHandler;
use crate::commands::CommandError;
use crate::commons::lazy::Lazy;
use crate::config::UiTheme;
use crate::config::{load_config, Config};
use crate::diff_cache::DiffCache;
use crate::tree::{FileTree, TreeNode};
use crate::view::View;
use crate::worker::{tasks, EventRegistry};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use syntect::highlighting::Theme;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

#[derive(TypedBuilder)]
#[builder(build_method(into = App))]
pub struct AppReq {
    #[builder(default)]
    pub theme: Lazy<UiTheme>,
    pub tree: Arc<RwLock<FileTree>>,
    pub view: View,
    pub syntax_theme: Arc<RwLock<Lazy<Theme>>>,
    pub config_error: Arc<RwLock<Option<String>>>,
    pub current_path: Arc<RwLock<Option<PathBuf>>>,
    pub config_path: PathBuf,
    pub worker: Arc<EventRegistry>,
    pub cache: DiffCache,
    pub handler: BoxedHandler,
    pub state: Arc<TuiState>,
    pub config: Config,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub base_path: Option<PathBuf>,
}

pub struct App {
    pub tree: Arc<RwLock<FileTree>>,
    pub cache: DiffCache,
    pub view: View,
    pub theme: Lazy<UiTheme>,
    pub syntax_theme: Arc<RwLock<Lazy<Theme>>>,
    pub config_path: PathBuf,
    pub config_error: Arc<RwLock<Option<String>>>,
    pub current_path: Arc<RwLock<Option<PathBuf>>>,
    pub worker: Arc<EventRegistry>,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
    pub base_path: Option<PathBuf>,

    pub handler: BoxedHandler,
    pub should_quit: bool,
    pub command_mode: CommandMode,
    pub state: Arc<TuiState>,
}

impl From<AppReq> for App {
    fn from(value: AppReq) -> Self {
        Self {
            tree: value.tree,
            cache: value.cache,
            view: value.view,
            theme: value.theme,
            syntax_theme: value.syntax_theme,
            config_path: value.config_path,
            current_path: value.current_path,
            worker: value.worker,
            left_path: value.left_path,
            right_path: value.right_path,
            base_path: value.base_path,
            state: value.state,
            should_quit: false,
            command_mode: CommandMode::Normal,
            config_error: value.config_error,
            handler: value.handler,
        }
    }
}

impl App {
    pub fn builder() -> AppReqBuilder {
        AppReq::builder()
    }

    pub async fn start(&mut self) -> Result<(), CommandError> {
        self.start_tree_calculation()?;
        self.start_config_watching(self.config_path.clone())?;
        self.run().await?;
        Ok(())
    }

    pub fn handle_reload_event(&mut self, path: &Path) {
        tracing::info!("Reloading config on main thread...");
        if let Err(e) = load_config(path, self.handler.clone()) {
            tracing::error!("Config compilation error: {}", e);
            *self.config_error.write() = Some(e.to_string());
        } else {
            *self.config_error.write() = None;
        }
    }

    pub async fn tick(&mut self) {
        while let Ok(event) = self.worker.try_recv() {
            if let crate::worker::Event::WatchConfigRes(res) = event {
                tracing::info!("Reloading config on main thread...");
                self.handle_reload_event(&res.path);
            }
        }

        {
            let theme_guard = self.state.theme.read();
            self.theme = Lazy::Ready(theme_guard.ui.clone());
            *self.syntax_theme.write() = Lazy::Ready(theme_guard.tm_theme.clone());
        }

        let current_path_val = self.view.file_view.read().current_path.clone();
        let mut path_guard = self.current_path.write();
        if *path_guard != current_path_val {
            *path_guard = current_path_val.clone();
            if let Some(path) = current_path_val {
                if matches!(
                    self.cache.diffs.get(&path),
                    crate::commons::lazy::Lazy::Uninitialized
                ) {
                    tracing::debug!(path = %path.display(), "Queueing full diff calculation");
                    self.cache.diffs.mark_started(path.clone());

                    let tree_read = self.tree.read();
                    fn find_file_paths_recursive(
                        nodes: &[TreeNode],
                        path: &std::path::Path,
                    ) -> Option<(Option<std::path::PathBuf>, Option<std::path::PathBuf>)>
                    {
                        for node in nodes {
                            match node {
                                TreeNode::File(f) => {
                                    if f.path == path {
                                        return Some((f.left_path.clone(), f.right_path.clone()));
                                    }
                                }
                                TreeNode::Directory(d) => {
                                    if let Some(paths) =
                                        find_file_paths_recursive(&d.children, path)
                                    {
                                        return Some(paths);
                                    }
                                }
                            }
                        }
                        None
                    }
                    if let Some((left_path, right_path)) =
                        find_file_paths_recursive(&tree_read.nodes, &path)
                    {
                        let _ = self.worker.send(tasks::full_diff::FullDiffReq {
                            node_path: path.clone(),
                            left_path,
                            right_path,
                        });
                    }
                }
            }
        }
        drop(path_guard); // Terminates immutable borrow of self before self.confirm_merge()

        if self.state.should_quit.load(Ordering::Relaxed) {
            self.should_quit = true;
        }

        {
            let mut cmd_mode_guard = self.state.command_mode.write();
            match *cmd_mode_guard {
                CommandMode::Normal => {}
                _ => {
                    self.command_mode =
                        std::mem::replace(&mut *cmd_mode_guard, CommandMode::Normal);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&self) {
        let _ = self.worker.shutdown().await;
        crate::config::clear_registry();
    }

    #[tracing::instrument(skip_all, fields(cmd = cmd))]
    pub fn execute_command(&mut self, cmd: &str) {
        let mut tree = self.tree.write();
        let view_read = self.view.tree_view.read();
        commands::execute(cmd, &mut tree, &view_read, &self.cache);
    }

    pub fn start_tree_calculation(&self) -> eyre::Result<()> {
        self.worker
            .send(tasks::calculate_file_tree::CalculateFileTreeReq {
                left: self.left_path.clone(),
                right: self.right_path.clone(),
            })?;
        Ok(())
    }

    pub fn start_config_watching(&self, path: PathBuf) -> eyre::Result<()> {
        self.worker.send(tasks::watch_config::WatchConfigReq {
            path,
            last_mtime: None,
        })?;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), crate::commands::CommandError> {
        tracing::debug!("Initializing terminal");
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        tracing::info!("Entering main event loop");
        let mut aborted = false;
        loop {
            self.tick().await;
            terminal.draw(|f| draw::draw(f, self))?;

            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    if let CommandMode::Active(ref mut buf) = self.command_mode {
                        if key.kind == event::KeyEventKind::Press
                            || key.kind == event::KeyEventKind::Repeat
                        {
                            match key.code {
                                KeyCode::Enter => {
                                    let cmd = buf.clone();
                                    self.execute_command(&cmd);
                                    self.command_mode = CommandMode::Normal;
                                }
                                KeyCode::Esc => {
                                    self.command_mode = CommandMode::Normal;
                                }
                                KeyCode::Backspace => {
                                    buf.pop();
                                }
                                KeyCode::Char(c) => {
                                    buf.push(c);
                                }
                                _ => {}
                            }
                        }
                    } else if let CommandMode::ConfirmMerge = self.command_mode {
                        if key.kind == event::KeyEventKind::Press {
                            match key.code {
                                KeyCode::Enter => {
                                    self.command_mode = CommandMode::Normal;
                                    self.handler.dispatch(&crate::actions::Action(
                                        crate::actions::Actions::global(
                                            crate::actions::GlobalActions::execute_merge,
                                        ),
                                    ));
                                }
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    self.command_mode = CommandMode::Normal;
                                }
                                _ => {}
                            }
                        }
                    } else {
                        // Standard keybind handling
                        let mut matched_targets = Vec::new();

                        let active_mode =
                            if *self.view.current.read() == crate::view::ViewKind::File {
                                crate::actions::keybinds::KeybindMode::View(
                                    crate::actions::keybinds::View::File,
                                )
                            } else {
                                crate::actions::keybinds::KeybindMode::View(
                                    crate::actions::keybinds::View::Tree,
                                )
                            };

                        crate::config::ACTIVE_REGISTRY.with(|r| {
                            let reg = r.borrow();
                            for (mode, kb, targets) in &reg.bindings {
                                if (*mode == crate::actions::keybinds::KeybindMode::Global
                                    || *mode == active_mode)
                                    && kb.matches(&key)
                                {
                                    matched_targets.extend(targets.clone());
                                }
                            }
                        });

                        if !matched_targets.is_empty() {
                            for target in matched_targets {
                                match target {
                                    crate::actions::keybinds::ActionTarget::Static(action) => {
                                        self.handler.dispatch(&action);

                                        // Clean abort hook during transition
                                        if let crate::actions::Action(
                                            crate::actions::Actions::global(
                                                crate::actions::GlobalActions::quit,
                                            ),
                                        ) = action
                                        {
                                            aborted = true;
                                            break;
                                        }
                                    }
                                    crate::actions::keybinds::ActionTarget::Dynamic(cb) => {
                                        tracing::debug!(
                                            "Matched script keybind, executing callback"
                                        );
                                        if let Err(e) = cb.call::<()>(()).into_result() {
                                            tracing::error!(
                                                "Script keybind execution error: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            if aborted {
                                break;
                            }
                        }
                    }
                }
            }

            if self.should_quit {
                break;
            }
        }

        tracing::debug!("Restoring terminal state");
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        tracing::info!("Shutting down background worker...");
        let _ = self.shutdown().await;

        if aborted {
            tracing::warn!("Application aborted.");
            return Err(crate::commands::CommandError::Aborted);
        }

        Ok(())
    }

    pub fn sync_cache_with_tree(&self) {
        let tree = self.tree.read();
        self.sync_cache_recursive(&tree.nodes);
    }

    fn sync_cache_recursive(&self, nodes: &[TreeNode]) {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.state == crate::tree::StagingState::Staged
                        || f.state == crate::tree::StagingState::Unstaged
                    {
                        let target_val = f.state == crate::tree::StagingState::Staged;

                        let mut diff_clone = None;
                        if let Some(val) = self.cache.diffs.get(&f.path).value() {
                            if let crate::diff::DiffResult::Text(diff) = val {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                let needs_sync = diff.line_selections.len() != total_lines
                                    || diff.line_selections.iter().any(|&v| v != target_val);

                                if needs_sync {
                                    diff_clone = Some(val.clone());
                                }
                            }
                        }

                        if let Some(mut diff_result) = diff_clone {
                            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                                let total_lines: usize =
                                    diff.hunks.iter().map(|h| h.lines.len()).sum();
                                diff.line_selections.clear();
                                diff.line_selections.resize(total_lines, target_val);
                            }
                            self.cache.diffs.set(f.path.clone(), diff_result);
                        }
                    }
                }
                TreeNode::Directory(d) => {
                    self.sync_cache_recursive(&d.children);
                }
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub fn toggle_stage_hunk(&mut self, hunk_idx: usize) {
        let Some(path) = self.view.file_view.read().current_path.clone() else {
            return;
        };

        let mut diff_clone = None;
        if let Some(val) = self.cache.diffs.get(&path).value() {
            diff_clone = Some(val.clone());
        }

        if let Some(mut diff_result) = diff_clone {
            if let crate::diff::DiffResult::Text(ref mut diff) = diff_result {
                let total_lines: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();

                let default_staged = self
                    .tree
                    .read()
                    .get_file_state(&path)
                    .unwrap_or(crate::tree::StagingState::Unstaged)
                    == crate::tree::StagingState::Staged;

                if diff.line_selections.len() != total_lines {
                    diff.line_selections.resize(total_lines, default_staged);
                }

                let mut start_idx = 0;
                for hunk in diff.hunks.iter().take(hunk_idx) {
                    start_idx += hunk.lines.len();
                }

                if let Some(hunk) = diff.hunks.get(hunk_idx) {
                    let mut all_staged = true;
                    for (j, line) in hunk.lines.iter().enumerate() {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        ) && !diff
                            .line_selections
                            .get(start_idx + j)
                            .copied()
                            .unwrap_or(default_staged)
                        {
                            all_staged = false;
                            break;
                        }
                    }

                    let new_state = !all_staged;
                    for (j, line) in hunk.lines.iter().enumerate() {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        ) && start_idx + j < diff.line_selections.len()
                        {
                            diff.line_selections[start_idx + j] = new_state;
                        }
                    }
                }

                let mut has_staged = false;
                let mut has_unstaged = false;
                let mut current_idx = 0;
                for h in &diff.hunks {
                    for line in &h.lines {
                        if matches!(
                            line,
                            crate::diff::DiffLine::Addition { .. }
                                | crate::diff::DiffLine::Deletion { .. }
                        ) {
                            let is_staged = diff
                                .line_selections
                                .get(current_idx)
                                .copied()
                                .unwrap_or(default_staged);
                            if is_staged {
                                has_staged = true;
                            } else {
                                has_unstaged = true;
                            }
                        }
                        current_idx += 1;
                    }
                }

                let new_staging_state = if has_staged && has_unstaged {
                    crate::tree::StagingState::PartiallyStaged
                } else if has_staged {
                    crate::tree::StagingState::Staged
                } else {
                    crate::tree::StagingState::Unstaged
                };

                self.update_file_state(&path, new_staging_state);
            }

            self.cache.diffs.set(path, diff_result);
        }
    }

    fn update_file_state(&mut self, path: &PathBuf, new_state: crate::tree::StagingState) {
        fn find_and_update(
            nodes: &mut [TreeNode],
            path: &PathBuf,
            new_state: crate::tree::StagingState,
        ) -> bool {
            for node in nodes {
                match node {
                    TreeNode::File(f) => {
                        if f.path == *path {
                            f.state = new_state;
                            return true;
                        }
                    }
                    TreeNode::Directory(d) => {
                        if find_and_update(&mut d.children, path, new_state) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        let mut tree = self.tree.write();
        find_and_update(&mut tree.nodes, path, new_state);
    }

    pub fn get_diff_summary(&self) -> (usize, usize, usize) {
        let (mut a, mut d, mut m) = (0, 0, 0);
        self.count_recursive(&self.tree.read().nodes, &mut a, &mut d, &mut m);
        (a, d, m)
    }

    pub fn get_merge_stats(
        &self,
    ) -> (
        (usize, usize, usize, usize, usize),
        (usize, usize, usize, usize, usize),
    ) {
        self.sync_cache_with_tree(); // Sync stale line_selections before reading
        let mut left = (0, 0, 0, 0, 0);
        let mut right = (0, 0, 0, 0, 0);
        self.count_split_recursive(&self.tree.read().nodes, &self.cache, &mut left, &mut right);
        (left, right)
    }

    fn count_split_recursive(
        &self,
        nodes: &[TreeNode],
        cache: &DiffCache,
        left: &mut (usize, usize, usize, usize, usize),
        right: &mut (usize, usize, usize, usize, usize),
    ) {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    let is_added = f.left_path.is_none();
                    let is_deleted = f.right_path.is_none();

                    if f.state == crate::tree::StagingState::Staged
                        || f.state == crate::tree::StagingState::PartiallyStaged
                    {
                        if is_added {
                            left.0 += 1;
                        } else if is_deleted {
                            left.1 += 1;
                        } else {
                            left.2 += 1;
                        }
                    }

                    if f.state == crate::tree::StagingState::Unstaged
                        || f.state == crate::tree::StagingState::PartiallyStaged
                    {
                        if is_added {
                            right.0 += 1;
                        } else if is_deleted {
                            right.1 += 1;
                        } else {
                            right.2 += 1;
                        }
                    }

                    if let Some(crate::diff::DiffResult::Text(diff)) =
                        cache.diffs.get(&f.path).value()
                    {
                        let mut selection_idx = 0;
                        let default_staged = f.state == crate::tree::StagingState::Staged;
                        for hunk in &diff.hunks {
                            for diff_line in &hunk.lines {
                                let is_staged = *diff
                                    .line_selections
                                    .get(selection_idx)
                                    .unwrap_or(&default_staged);
                                selection_idx += 1;
                                match diff_line {
                                    crate::diff::DiffLine::Addition { .. } => {
                                        if is_staged {
                                            left.3 += 1;
                                        } else {
                                            right.3 += 1;
                                        }
                                    }
                                    crate::diff::DiffLine::Deletion { .. } => {
                                        if is_staged {
                                            left.4 += 1;
                                        } else {
                                            right.4 += 1;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    } else {
                        let mut ins = 0;
                        let mut del = 0;
                        let mut stats_found = false;
                        if let Some(crate::diff::DiffStats::Text {
                            insertions,
                            deletions,
                        }) = cache.stats.get(&f.path).value()
                        {
                            ins = *insertions;
                            del = *deletions;
                            stats_found = true;
                        }

                        if !stats_found {
                            if is_added {
                                if let Some(r) = &f.right_path {
                                    if let Ok(content) = std::fs::read_to_string(r) {
                                        ins = content.lines().count();
                                    }
                                }
                            } else if is_deleted {
                                if let Some(l) = &f.left_path {
                                    if let Ok(content) = std::fs::read_to_string(l) {
                                        del = content.lines().count();
                                    }
                                }
                            }
                        }

                        if f.state == crate::tree::StagingState::Staged {
                            left.3 += ins;
                            left.4 += del;
                        } else if f.state == crate::tree::StagingState::Unstaged {
                            right.3 += ins;
                            right.4 += del;
                        } else {
                            left.3 += ins;
                            left.4 += del;
                        }
                    }
                }
                TreeNode::Directory(dir) => {
                    self.count_split_recursive(&dir.children, cache, left, right)
                }
            }
        }
    }

    fn count_recursive(&self, nodes: &[TreeNode], a: &mut usize, d: &mut usize, m: &mut usize) {
        for node in nodes {
            match node {
                TreeNode::File(f) => {
                    if f.left_path.is_none() {
                        *a += 1;
                    } else if f.right_path.is_none() {
                        *d += 1;
                    } else {
                        *m += 1;
                    }
                }
                TreeNode::Directory(dir) => self.count_recursive(&dir.children, a, d, m),
            }
        }
    }
}
