//! Syntax highlighting: queueing, spawn_blocking computation, theme invalidation.
use std::path::PathBuf;
use std::sync::Arc;

use crate::diff::DiffResult;
use crate::diff_cache::{DiffCache, SideTokens};
use crate::syntax::SyntaxEngine;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::EventSender;
use crate::worker::events::diff_update::DiffUpdate;
use crate::worker::events::file_hovered::FileHovered;
use crate::worker::events::file_opened::FileOpened;
use crate::worker::events::syntax_res::SyntaxRes;
use crate::worker::events::theme_update::ThemeUpdate;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;

pub struct Syntax;

#[derive(Clone)]
pub struct SyntaxReq {
    pub node_path: PathBuf,
    pub text: Arc<str>,
    /// The old side of the diff: deletions keep their syntax fg through it.
    pub old_text: Option<Arc<str>>,
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
        if ctx.cache.syntax.get(&path).is_some() {
            tracing::trace!(node_path = %path.display(), "Syntax is already cached");
            return Ok(());
        }

        if let DiffResult::Text(ref file_diff) = diff_result {
            let text = file_diff.new_file_content.clone();
            // Old side only when it exists (new files have none).
            let old_text = (!file_diff.old_file_content.is_empty())
                .then(|| file_diff.old_file_content.clone());

            if let Some(generation) = ctx.cache.syntax.mark_started(&path) {
                tracing::trace!(node_path = %path.display(), "Queueing Syntax task");
                if tx
                    .send(SyntaxReq {
                        node_path: path.clone(),
                        text,
                        old_text,
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

    fn queue_file(path: PathBuf, ctx: &SyntaxContext, tx: &EventSender) -> eyre::Result<()> {
        // Queue full diff calculation if not already cached or in flight. Its
        // DiffUpdate re-enters this listener and queues syntax afterward.
        let diff_result = match ctx.cache.diffs.get(&path) {
            Some(diff_result) => diff_result,
            None => {
                let claim = ctx.cache.diffs.mark_started(&path);
                if let Some(generation) = claim {
                    // Do not hold the tree lock while calling the cache.
                    let paths = ctx.tree.read().find_paths(&path);
                    match paths {
                        Some((left_path, right_path)) => {
                            let req = crate::worker::tasks::full_diff::FullDiffReq {
                                node_path: path.clone(),
                                left_path,
                                right_path,
                                generation,
                            };
                            if tx.send(req).is_err() {
                                ctx.cache.diffs.cancel(&path, generation);
                            }
                        }
                        None => {
                            ctx.cache.diffs.cancel(&path, generation);
                            tracing::warn!(
                                node_path = %path.display(),
                                "No diff paths for file; syntax highlighting skipped"
                            );
                        }
                    }
                } else {
                    tracing::trace!(node_path = %path.display(), "Full diff already in flight");
                }
                return Ok(());
            }
        };

        Self::try_queue_syntax_task(path, ctx, tx, &diff_result)
    }
}

impl Listener<SyntaxReq> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(event: SyntaxReq, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        tracing::debug!("Computing syntax highlighting");

        let theme = ctx.theme.read().tm_theme.clone();
        let engine = ctx.engine.clone();
        let node_path = event.node_path.clone();
        let text = event.text.clone();
        let old_text = event.old_text.clone();
        let generation = event.generation;

        let highlighted = tokio::task::spawn_blocking({
            let node_path = node_path.clone();
            move || {
                let new = Self::highlight_blocking(&engine, theme.as_ref(), &node_path, &text);
                let old = old_text
                    .map(|old| Self::highlight_blocking(&engine, theme.as_ref(), &node_path, &old))
                    .unwrap_or_default();
                SideTokens {
                    old: Arc::new(old),
                    new: Arc::new(new),
                }
            }
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

        if ctx
            .cache
            .syntax
            .set(node_path.clone(), Arc::new(highlighted), generation)
        {
            // Highlighting is cached but nothing else announces it: tell the
            // app to repaint so the first paint of a file is not colorless.
            let _ = tx.send(SyntaxRes { path: node_path });
        } else {
            // The cache moved under us (a clear bumped the generation between
            // the claim and completion). Re-queue once: the open file's
            // highlight must return without user interaction.
            tracing::debug!(
                node_path = %node_path.display(),
                "syntax result rejected as stale; re-queueing"
            );
            if let Some(generation) = ctx.cache.syntax.mark_started(&node_path) {
                if tx
                    .send(SyntaxReq {
                        node_path: node_path.clone(),
                        text: event.text,
                        old_text: event.old_text,
                        generation,
                    })
                    .is_err()
                {
                    ctx.cache.syntax.cancel(&node_path, generation);
                }
            }
        }

        Ok(())
    }
}

impl Listener<FileOpened> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: FileOpened, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        Self::queue_file(event.path, &ctx, &tx)
    }
}

impl Listener<FileHovered> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: FileHovered, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // Cursor movement is frequent. Debounce off the worker and discard the
        // request if selection moved again; no timer or parsing runs on the UI
        // thread.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if ctx.cache.preload_generation() != event.generation {
            return Ok(());
        }
        Self::queue_file(event.path, &ctx, &tx)
    }
}

impl Listener<DiffUpdate> for Syntax {
    type Sender = EventSender;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.path.display()))]
    async fn handle(event: DiffUpdate, ctx: Self::Context, tx: EventSender) -> eyre::Result<()> {
        // The diff was written to the cache before this event was emitted; a
        // miss means it was invalidated or cleared since, so there is nothing
        // to highlight.
        let Some(diff_result) = ctx.cache.diffs.get(&event.path) else {
            tracing::debug!(
                node_path = %event.path.display(),
                "Diff gone from cache before syntax task was queued"
            );
            return Ok(());
        };
        Self::try_queue_syntax_task(event.path, &ctx, &tx, &diff_result)
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
