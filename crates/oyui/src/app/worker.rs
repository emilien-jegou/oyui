use core_lib::diff_cache::DiffCache;
use core_lib::worker::{AsyncWorkerEvent, WorkerRequest};
use crossbeam_channel::{Receiver, Sender};

pub fn process_events(
    rx: &Receiver<AsyncWorkerEvent>,
    tx: &Sender<WorkerRequest>,
    cache: &mut DiffCache,
) {
    while let Ok(event) = rx.try_recv() {
        match event {
            AsyncWorkerEvent::DiffStatsReady(path, stats) => {
                cache.stats.set(path, stats);
            }
            AsyncWorkerEvent::FullDiffReady(path, diff, right_path) => {
                let text = diff.new_text.clone();
                cache.diffs.set(path.clone(), diff);
                cache.syntax.mark_started(path.clone());
                let _ = tx.send(WorkerRequest::ComputeSyntax {
                    node_path: path,
                    text,
                    right_path,
                });
            }
            AsyncWorkerEvent::SyntaxReady(path, syntax) => {
                cache.syntax.set(path, syntax);
            }
        }
    }
}
