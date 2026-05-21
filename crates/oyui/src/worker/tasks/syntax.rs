use std::path::PathBuf;
use std::sync::Arc;

use crate::syntax::SyntaxEngine;
use oyui_tasker::{TaskerContext, WorkerTask};
use syntect::highlighting::Style as SyntectStyle;

pub struct Syntax;

#[derive(Debug, Clone)]
pub struct SyntaxReq {
    pub node_path: PathBuf,
    pub text: Arc<str>,
    pub right_path: PathBuf,
}

#[derive(Debug)]
pub struct SyntaxRes {
    pub node_path: PathBuf,
    pub highlighted: Vec<Vec<(SyntectStyle, String)>>,
}

#[derive(TaskerContext)]
pub struct SyntaxContext {
    engine: SyntaxEngine,
}

impl WorkerTask for Syntax {
    type Request = SyntaxReq;
    type Response = SyntaxRes;
    type Context = SyntaxContext;

    #[tracing::instrument(skip_all, fields(node_path = %req.node_path.display()))]
    async fn handle(req: Self::Request, ctx: Self::Context) -> Self::Response {
        tracing::debug!("Computing syntax highlighting");

        let syntax_set = &ctx.engine.syntax_set;
        let theme = &ctx.engine.theme;
        let syntax = syntax_set
            .find_syntax_by_extension(
                req.right_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or(""),
            )
            .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

        let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
        let highlighted: Vec<Vec<_>> = req
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
        SyntaxRes {
            node_path: req.node_path,
            highlighted,
        }
    }
}
