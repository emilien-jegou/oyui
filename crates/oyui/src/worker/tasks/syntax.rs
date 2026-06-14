use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::actions::state::TuiState;
use crate::diff::DiffResult;
use crate::syntax::SyntaxEngine;
use crate::view::View;
use crate::worker::events::diff_update::DiffUpdate;
use crate::worker::events::file_opened::FileOpened;
use crate::worker::events::file_syntax_update::FileSyntaxUpdate;
use crate::worker::events::theme_update::ThemeUpdate;
use crate::worker::EventSender;
use crate::{commons::lazy::CacheVersion, diff_cache::DiffCache};
use oyui_tasker::{Listener, TaskerContext};

pub struct Syntax;

#[derive(Clone)]
pub struct SyntaxReq {
    pub node_path: PathBuf,
    pub text: Arc<str>,
    pub version: CacheVersion,
}

impl fmt::Debug for SyntaxReq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntaxReq")
            .field("version", &self.version)
            .finish()
    }
}

#[derive(TaskerContext)]
pub struct SyntaxContext {
    cache: DiffCache,
    view: View,
    engine: SyntaxEngine,
    state: Arc<TuiState>,
}

impl Syntax {
    /// Attempts to mark the path as started and queues the syntax highlighting task
    /// if the file state allows it (i.e., not already running or up-to-date).
    fn try_queue_syntax_task(
        path: PathBuf,
        ctx: &SyntaxContext,
        tx: &EventSender,
        diff_result: &DiffResult,
        force_new_file_gen: bool,
    ) -> eyre::Result<()> {
        if let DiffResult::Text(ref file_diff) = diff_result {
            let text = file_diff.new_file_content.clone();

            if let Some(version) = ctx
                .cache
                .syntax
                .mark_started(path.clone(), force_new_file_gen)
            {
                tracing::trace!(node_path = %path.display(), "Queueing Syntax task");
                let _ = tx.send(SyntaxReq {
                    node_path: path,
                    text,
                    version,
                });
            } else {
                tracing::trace!(node_path = %path.display(), "Syntax task is already running or up to date");
            }
        }

        Ok(())
    }
}

impl Listener<SyntaxReq, EventSender> for Syntax {
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(
        event: SyntaxReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Computing syntax highlighting");

        let theme_guard = ctx.state.theme.read();
        let theme = theme_guard.tm_theme.clone();
        drop(theme_guard);

        let syntax_set = &ctx.engine.syntax_set;
        let syntax = syntax_set
            .find_syntax_by_extension(
                event
                    .node_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            )
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let highlighted: Vec<Vec<_>> = if let Some(ref theme) = theme {
            let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
            event
                .text
                .lines()
                .map(|line| {
                    highlighter
                        .highlight_line(line, syntax_set)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(style, token)| (style, token.to_string()))
                        .collect()
                })
                .collect()
        } else {
            Vec::new()
        };

        tracing::trace!("Syntax highlighting finished");

        let committed = ctx.cache.syntax.set_versioned(
            event.node_path.clone(),
            event.version,
            highlighted.clone(),
        );

        if committed {
            tx.send(FileSyntaxUpdate {
                node_path: event.node_path,
                highlighted,
            })?;
        } else {
            tracing::debug!("Syntax computation discarded: stale generation or content update");
        }

        Ok(())
    }
}

impl Listener<FileOpened, EventSender> for Syntax {
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: FileOpened, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        let df_r = ctx.cache.diffs.get(&event.path);

        let diff_result = df_r
            .value()
            .ok_or_else(|| eyre::eyre!("No diff for file: {}", event.path.display()))?;

        // Reopening a file doesn't modify content, so we do not force a new file generation
        Self::try_queue_syntax_task(event.path, &ctx, &tx, diff_result, false)
    }
}

impl Listener<DiffUpdate, EventSender> for Syntax {
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: DiffUpdate, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // Content changed, so we increment the file generation
        Self::try_queue_syntax_task(
            event.path.to_path_buf(),
            &ctx,
            &tx,
            &event.diff_result,
            true,
        )
    }
}

impl Listener<ThemeUpdate, EventSender> for Syntax {
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all)]
    async fn handle(_: ThemeUpdate, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        ctx.cache.syntax.invalidate_all();

        // Update syntax theme for file in view since FileOpened won't retrigger.
        let file_view = ctx.view.file_view.read();
        let path = match &file_view.current_path {
            Some(v) => v.clone(),
            None => {
                tracing::debug!("No file in view, no reload needed");
                return Ok(());
            }
        };
        drop(file_view);

        let df_r = ctx.cache.diffs.get(&path);

        let diff_result = df_r
            .value()
            .ok_or_else(|| eyre::eyre!("No diff for file: {}", path.display()))?;

        // Theme updates do not modify content, so we do not force a new file generation
        Self::try_queue_syntax_task(path, &ctx, &tx, diff_result, false)
    }
}
