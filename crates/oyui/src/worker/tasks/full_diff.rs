use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::WorkerTask;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use crate::diff::{FileDiff, Hunk};

pub struct FullDiff;

#[derive(Debug, Clone)]
pub struct FullDiffReq {
    pub node_path: PathBuf,
    pub left_path: PathBuf,
    pub right_path: PathBuf,
}

pub struct FullDiffRes {
    pub node_path: PathBuf,
    pub file_diff: FileDiff,
    pub right_path: PathBuf,
}

impl WorkerTask for FullDiff {
    type Request = FullDiffReq;
    type Response = FullDiffRes;
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

        let mut hunks = Vec::new();
        for hunk in diff.hunks() {
            hunks.push(Hunk {
                before_lines: (hunk.before.start as usize)..(hunk.before.end as usize),
                after_lines: (hunk.after.start as usize)..(hunk.after.end as usize),
            });
        }

        let file_diff = FileDiff {
            old_text: Arc::from(left_text),
            new_text: Arc::from(right_text),
            hunks,
            line_selections: Default::default(),
        };

        FullDiffRes {
            node_path: req.node_path,
            file_diff,
            right_path: req.right_path,
        }
    }
}
