use crate::diff_cache::DiffCache;
use crate::worker::{tasks, Tasker, WorkerEvent};

pub async fn process_events(worker: &mut Tasker, cache: &mut DiffCache) {
    if let Some(event) = worker.recv().await {
        match event {
            WorkerEvent::Stats(res) => {
                cache.stats.set(res.node_path, res.stats);
            }
            WorkerEvent::FullDiff(res) => {
                let text = res.file_diff.new_text.clone();
                cache.diffs.set(res.node_path.clone(), res.file_diff);
                cache.syntax.mark_started(res.node_path.clone());

                let _ = worker.send(tasks::syntax::SyntaxReq {
                    node_path: res.node_path,
                    text,
                    right_path: res.right_path,
                });
            }
            WorkerEvent::Syntax(res) => {
                cache.syntax.set(res.node_path, res.highlighted);
            }
        }
    }
}
