use tokio::sync::mpsc::error::TryRecvError;

use crate::{
    diff_cache::DiffCache,
    worker::{tasks, Tasker, WorkerEvent},
};

pub async fn process_events(worker: &mut Tasker, cache: &mut DiffCache) {
    match worker.try_recv() {
        Ok(event) => match event {
            WorkerEvent::Stats(res) => {
                tracing::debug!("Applied Stats cache");
                for (node_path, stats) in res.stats {
                    cache.stats.set(node_path, stats);
                }
            }
            WorkerEvent::FullDiff(res) => {
                tracing::debug!(node_path = %res.node_path.display(), "Applied FullDiff cache");
                let text = res.file_diff.new_text.clone();
                cache.diffs.set(res.node_path.clone(), res.file_diff);
                cache.syntax.mark_started(res.node_path.clone());

                tracing::trace!(node_path = %res.node_path.display(), "Queueing Syntax task");
                let _ = worker.send(tasks::syntax::SyntaxReq {
                    node_path: res.node_path,
                    text,
                    right_path: res.right_path,
                });
            }
            WorkerEvent::Syntax(res) => {
                tracing::debug!(node_path = %res.node_path.display(), "Applied Syntax cache");
                cache.syntax.set(res.node_path, res.highlighted);
            }
        },
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            tracing::error!("Worker channel disconnected unexpectedly.");
        }
    }
}
