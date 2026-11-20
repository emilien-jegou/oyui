//! Directory diff walk task and its tree-installation listener.
use crate::tree::FileTree;
use oyui_tasker::{Listener, TaskerContext};
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

pub struct CalculateFileTree;

#[derive(Clone)]
pub struct CalculateFileTreeReq {
    pub left: PathBuf,
    pub right: PathBuf,
    /// When set, every discovered change starts staged (kept on confirm).
    pub default_staged: bool,
}

#[derive(Clone)]
pub struct CalculateFileTreeRes {
    pub tree: FileTree,
    pub files_to_stat: Vec<(PathBuf, PathBuf, PathBuf)>,
}

impl Listener<CalculateFileTreeReq> for CalculateFileTree {
    type Sender = crate::worker::EventSender;
    type Context = ();

    #[tracing::instrument(skip_all, fields(left = %event.left.display(), right = %event.right.display()))]
    async fn handle(
        event: CalculateFileTreeReq,
        _ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Calculating file tree...");
        let left = event.left;
        let right = event.right;
        let default_staged = event.default_staged;
        let (mut tree, files_to_stat) =
            tokio::task::spawn_blocking(move || FileTree::build_from_dir_diff(&left, &right))
                .await
                .map_err(|e| eyre::eyre!("file tree task panicked: {e}"))?;
        if default_staged {
            for file in tree.files_mut() {
                file.state = crate::tree::StagingState::Staged;
            }
        }
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
impl Listener<CalculateFileTreeRes> for CalculateFileTreeResListener {
    type Sender = crate::worker::EventSender;
    type Context = CalcTreeResCtx;

    async fn handle(
        event: CalculateFileTreeRes,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        // The emptiness verdict is this listener's only special case: an
        // empty tree is still installed (the TUI must render the empty
        // state) but no stats are computed over nothing. The app separately
        // reacts to the same shape by surfacing "no modifications" and
        // exiting; here the only decision is whether the stats chain runs.
        let empty = event.tree.nodes.is_empty();
        ctx.tree.write().replace(event.tree);
        if empty {
            tracing::info!("No modifications found.");
            return Ok(());
        }
        let _ = tx.send(crate::worker::tasks::stats::StatsReq {
            files: event.files_to_stat,
        });
        Ok(())
    }
}
