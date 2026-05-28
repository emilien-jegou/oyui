use std::path::PathBuf;
use std::sync::Arc;

use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use syntect::highlighting::Style as SyntectStyle;

pub struct Syntax;

#[derive(Debug, Clone)]
pub struct SyntaxReq {
    pub node_path: PathBuf,
    pub text: Arc<str>,
    pub right_path: Option<PathBuf>,
    pub theme: Arc<syntect::highlighting::Theme>,
}

#[derive(Debug, Clone)]
pub struct SyntaxRes {
    pub node_path: PathBuf,
    pub highlighted: Vec<Vec<(SyntectStyle, String)>>,
}

#[derive(TaskerContext)]
pub struct SyntaxContext {
    engine: SyntaxEngine,
}

impl Listener<SyntaxReq, crate::worker::EventSender> for Syntax {
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %event.node_path.display()))]
    async fn handle(
        event: SyntaxReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Computing syntax highlighting");

        let syntax_set = &ctx.engine.syntax_set;
        let theme = &event.theme;
        let syntax = syntax_set
            .find_syntax_by_extension(
                event
                    .right_path
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            )
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
        let highlighted: Vec<Vec<_>> = event
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
            .collect();

        tracing::trace!("Syntax highlighting finished");
        tx.send(SyntaxRes {
            node_path: event.node_path.clone(),
            highlighted,
        })?;
        Ok(())
    }
}

#[derive(TaskerContext)]
pub struct SyntaxResCtx {
    pub cache: Arc<RwLock<DiffCache>>,
}

pub struct SyntaxResListener;
impl Listener<SyntaxRes, crate::worker::EventSender> for SyntaxResListener {
    type Context = SyntaxResCtx;

    async fn handle(
        event: SyntaxRes,
        ctx: Self::Context,
        _tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!(node_path = %event.node_path.display(), "Applied Syntax cache");
        ctx.cache.write().syntax.set(event.node_path, event.highlighted);
        Ok(())
    }
}
