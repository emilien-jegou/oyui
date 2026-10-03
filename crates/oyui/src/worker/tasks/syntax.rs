//! Syntax highlighting: queueing, spawn_blocking computation, theme invalidation.
use std::path::PathBuf;
use std::sync::Arc;

use crate::diff::DiffResult;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::events::diff_update::DiffUpdate;
use crate::worker::events::file_opened::FileOpened;
use crate::worker::events::theme_update::ThemeUpdate;
use crate::worker::EventSender;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;

pub struct Syntax;

#[derive(Clone)]
pub struct SyntaxReq {
    pub node_path: PathBuf,
    pub text: Arc<str>,
    /// Cache generation this task's claim was made under.
    pub generation: u64,
}

#[derive(TaskerContext)]
pub struct SyntaxContext {
    cache: DiffCache,
    engine: SyntaxEngine,
    theme: Arc<RwLock<ThemeState>>,
    tree: Arc<RwLock<FileTree>>,
}

impl Syntax {
    /// Highlights `text` for `node_path` using the syntect engine and optional theme.
    fn highlight_blocking(
        engine: &SyntaxEngine,
        theme: Option<&syntect::highlighting::Theme>,
        node_path: &std::path::Path,
        text: &str,
    ) -> Vec<Vec<(syntect::highlighting::Style, String)>> {
        let syntax_set = &engine.syntax_set;
        let syntax = syntax_set
            .find_syntax_by_extension(node_path.extension().and_then(|s| s.to_str()).unwrap_or(""))
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        match theme {
            Some(theme) => {
                let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
                text.lines()
                    .map(|line| {
                        highlighter
                            .highlight_line(line, syntax_set)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(style, token)| (style, token.to_string()))
                            .collect()
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Attempts to mark the path as in-flight and queues the syntax highlighting task
    /// if the file state allows it (i.e., not already running).
    fn try_queue_syntax_task(
        path: PathBuf,
        ctx: &SyntaxContext,
        tx: &EventSender,
        diff_result: &DiffResult,
    ) -> eyre::Result<()> {
        if let DiffResult::Text(ref file_diff) = diff_result {
            let text = file_diff.new_file_content.clone();

            if let Some(generation) = ctx.cache.syntax.mark_started(&path) {
                tracing::trace!(node_path = %path.display(), "Queueing Syntax task");
                if tx
                    .send(SyntaxReq {
                        node_path: path.clone(),
                        text,
                        generation,
                    })
                    .is_err()
                {
                    ctx.cache.syntax.cancel(&path, generation);
                }
            } else {
                tracing::trace!(node_path = %path.display(), "Syntax task is already running");
            }
        }

        Ok(())
    }
}

impl Listener<SyntaxReq> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(
        event: SyntaxReq,
        ctx: Self::Context,
        _tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Computing syntax highlighting");

        let theme = ctx.theme.read().tm_theme.clone();
        let engine = ctx.engine.clone();
        let node_path = event.node_path;
        let text = event.text;
        let generation = event.generation;

        let highlighted = tokio::task::spawn_blocking({
            let node_path = node_path.clone();
            move || Self::highlight_blocking(&engine, theme.as_ref(), &node_path, &text)
        })
        .await;

        let highlighted = match highlighted {
            Ok(highlighted) => highlighted,
            Err(join_err) => {
                // Release the in-flight mark so the path can be retried.
                ctx.cache.syntax.cancel(&node_path, generation);
                tracing::error!(?join_err, "syntax highlighting task panicked");
                return Err(eyre::eyre!("syntax highlighting task panicked: {join_err}"));
            }
        };

        tracing::trace!("Syntax highlighting finished");

        if !ctx
            .cache
            .syntax
            .set(node_path, Arc::new(highlighted), generation)
        {
            tracing::debug!("Discarded stale syntax result (cache was cleared)");
        }

        Ok(())
    }
}

impl Listener<FileOpened> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: FileOpened, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // Queue full diff calculation if not already cached or in flight; the
        // resulting DiffUpdate event re-enters this listener, so return early.
        let diff_result = match ctx.cache.diffs.get(&event.path) {
            Some(diff_result) => diff_result,
            None => {
                // Atomically claim the computation: concurrent FileOpened events
                // for the same path only queue one FullDiff task.
                let claim = ctx.cache.diffs.mark_started(&event.path);
                if let Some(generation) = claim {
                    // Bind before matching: the tree guard in the scrutinee
                    // must not stay alive across the cache calls below.
                    let paths = ctx.tree.read().find_paths(&event.path);
                    match paths {
                        Some((left_path, right_path)) => {
                            let req = crate::worker::tasks::full_diff::FullDiffReq {
                                node_path: event.path.clone(),
                                left_path,
                                right_path,
                                generation,
                            };
                            if tx.send(req).is_err() {
                                ctx.cache.diffs.cancel(&event.path, generation);
                            }
                        }
                        None => {
                            ctx.cache.diffs.cancel(&event.path, generation);
                            tracing::warn!(
                                node_path = %event.path.display(),
                                "No diff paths for file; syntax highlighting skipped"
                            );
                        }
                    }
                } else {
                    tracing::trace!(
                        node_path = %event.path.display(),
                        "Full diff already in flight"
                    );
                }
                return Ok(());
            }
        };

        Self::try_queue_syntax_task(event.path, &ctx, &tx, &diff_result)
    }
}

impl Listener<DiffUpdate> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: DiffUpdate, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        Self::try_queue_syntax_task(event.path.to_path_buf(), &ctx, &tx, &event.diff_result)
    }
}

impl Listener<ThemeUpdate> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all)]
    async fn handle(event: ThemeUpdate, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // Clear syntax cache on theme change
        ctx.cache.syntax.clear();

        // The event carries the open file: reload its syntax since FileOpened
        // won't retrigger.
        let Some(path) = (match event {
            ThemeUpdate::Full(_, _, open) | ThemeUpdate::Tm(_, open) => open,
        }) else {
            tracing::debug!("No file in view, no reload needed");
            return Ok(());
        };

        let diff_result = match ctx.cache.diffs.get(&path) {
            Some(diff_result) => diff_result,
            None => {
                tracing::debug!(node_path = %path.display(), "No diff cached yet; syntax reload deferred");
                return Ok(());
            }
        };

        Self::try_queue_syntax_task(path, &ctx, &tx, &diff_result)
    }
}
