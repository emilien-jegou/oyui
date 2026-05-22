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
    pub left_path: Option<PathBuf>,
    pub right_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct FullDiffRes {
    pub node_path: PathBuf,
    pub file_diff: FileDiff,
    pub right_path: Option<PathBuf>,
}

async fn read_file_safe(path: &PathBuf, side: &str) -> String {
    match fs::read(path).await {
        Ok(bytes) => {
            match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => {
                    tracing::warn!(path = %path.display(), side, "File is not valid UTF-8 (likely binary)");
                    format!("// [oyui] Binary or invalid UTF-8 file content cannot be displayed for {}\n", side)
                }
            }
        }
        Err(e) => {
            tracing::error!(path = %path.display(), error = %e, side, "Failed to read file");
            format!("// [oyui] Error reading file: {}\n", e)
        }
    }
}

impl WorkerTask for FullDiff {
    type Request = FullDiffReq;
    type Response = FullDiffRes;
    type Context = ();

    #[tracing::instrument(skip_all, fields(node_path = %req.node_path.display()))]
    async fn handle(req: Self::Request, _ctx: Self::Context) -> Self::Response {
        tracing::debug!(
            left_path = ?req.left_path,
            right_path = ?req.right_path,
            "Computing full diff"
        );

        let left_fut = async {
            if let Some(p) = &req.left_path {
                read_file_safe(p, "left").await
            } else {
                String::new()
            }
        };

        let right_fut = async {
            if let Some(p) = &req.right_path {
                read_file_safe(p, "right").await
            } else {
                String::new()
            }
        };

        let (left_text, right_text) = tokio::join!(left_fut, right_fut);

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

        tracing::trace!("Full diff computation finished");
        FullDiffRes {
            node_path: req.node_path,
            file_diff,
            right_path: req.right_path,
        }
    }
}
