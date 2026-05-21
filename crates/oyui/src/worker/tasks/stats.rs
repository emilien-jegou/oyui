use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::WorkerTask;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

use crate::diff::DiffStats;

pub struct Stats;

#[derive(Debug, Clone)]
pub struct StatsReq {
    pub files: Vec<(PathBuf, PathBuf, PathBuf)>,
}

#[derive(Debug)]
pub struct StatsRes {
    pub stats: Vec<(PathBuf, DiffStats)>,
}

impl WorkerTask for Stats {
    type Request = StatsReq;
    type Response = StatsRes;
    type Context = ();

    #[tracing::instrument(skip_all)]
    async fn handle(req: Self::Request, _ctx: Self::Context) -> Self::Response {
        tracing::debug!("Computing diff stats for {} files", req.files.len());

        let stats = tokio::task::spawn_blocking(move || {
            req.files
                .into_par_iter()
                .map(|(node_path, left_path, right_path)| {
                    let left_text = fs::read_to_string(&left_path).unwrap_or_default();
                    let right_text = fs::read_to_string(&right_path).unwrap_or_default();

                    let input = InternedInput::new(left_text.as_str(), right_text.as_str());
                    let diff = Diff::compute(Algorithm::Myers, &input);

                    let stats = DiffStats {
                        insertions: diff.count_additions() as usize,
                        deletions: diff.count_removals() as usize,
                    };

                    (node_path, stats)
                })
                .collect::<Vec<_>>()
        })
        .await
        .expect("spawn_blocking panicked");

        tracing::trace!("Batch diff stats computation finished");
        StatsRes { stats }
    }
}
