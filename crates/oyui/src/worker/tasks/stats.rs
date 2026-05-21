use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::WorkerTask;
use std::path::PathBuf;
use tokio::fs;

use crate::diff::DiffStats;

pub struct Stats;

#[derive(Debug, Clone)]
pub struct StatsReq {
    pub node_path: PathBuf,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
}

pub struct StatsRes {
    pub node_path: PathBuf,
    pub stats: DiffStats,
}

impl WorkerTask for Stats {
    type Request = StatsReq;
    type Response = StatsRes;
    type Context = ();

    async fn handle(req: Self::Request, _ctx: Self::Context) -> Self::Response {
        let (left_res, right_res) = tokio::join!(
            fs::read_to_string(&req.left_path),
            fs::read_to_string(&req.right_path)
        );
        let left_text = left_res.unwrap_or_default();
        let right_text = right_res.unwrap_or_default();

        let input = InternedInput::new(left_text.as_str(), right_text.as_str());
        let diff = Diff::compute(Algorithm::Histogram, &input);

        let stats = DiffStats {
            insertions: diff.count_additions() as usize,
            deletions: diff.count_removals() as usize,
        };

        StatsRes {
            node_path: req.node_path,
            stats,
        }
    }
}
