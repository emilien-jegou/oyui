use crate::tree::FileTree;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub struct CalculateFileTree;

#[derive(Debug, Clone)]
pub struct CalculateFileTreeReq {
    pub left: PathBuf,
    pub right: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CalculateFileTreeRes {
    pub tree: FileTree,
    pub files_to_stat: Vec<(PathBuf, PathBuf, PathBuf)>,
}

impl Listener<CalculateFileTreeReq, crate::worker::EventSender> for CalculateFileTree {
    type Context = ();

    #[tracing::instrument(skip_all, fields(left = %event.left.display(), right = %event.right.display()))]
    async fn handle(
        event: CalculateFileTreeReq,
        _ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Calculating file tree...");
        let (tree, files_to_stat) = FileTree::build_from_dir_diff(&event.left, &event.right);
        tx.send(CalculateFileTreeRes {
            tree,
            files_to_stat,
        })?;
        Ok(())
    }
}

#[derive(TaskerContext)]
pub struct CalcTreeResCtx {
    pub tree: Arc<RwLock<FileTree>>,
    pub config_error: Arc<RwLock<Option<String>>>,
}

pub struct CalculateFileTreeResListener;
impl Listener<CalculateFileTreeRes, crate::worker::EventSender> for CalculateFileTreeResListener {
    type Context = CalcTreeResCtx;

    async fn handle(
        event: CalculateFileTreeRes,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        if event.tree.nodes.is_empty() {
            tracing::error!("No modifications found between directories. Nothing to split.");
            *ctx.config_error.write() =
                Some("No modifications found between directories. Nothing to split.".into());
        } else {
            *ctx.tree.write() = event.tree;
            let _ = tx.send(crate::worker::tasks::stats::StatsReq {
                files: event.files_to_stat,
            });
        }
        Ok(())
    }
}
