use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::WorkerTask;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use crate::diff::{DiffResult, FileDiff, Hunk};

const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1 MB limit

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
    pub result: DiffResult,
    pub right_path: Option<PathBuf>,
}

async fn load_text_safely(path: &PathBuf) -> Result<String, DiffResult> {
    // 1. Fetch metadata to check size before doing any expensive I/O
    let meta = match fs::metadata(path).await {
        Ok(m) => m,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    let size = meta.len();

    // 2. Protect against massive files (OOM/CPU lockup prevention)
    if size > MAX_FILE_SIZE {
        return Err(DiffResult::TooLarge(size));
    }

    // 3. Since we know the file is <= 1MB, it's safe to read fully into memory
    let buffer = match fs::read(path).await {
        Ok(b) => b,
        Err(e) => return Err(DiffResult::Error(e.to_string())),
    };

    // Helper closure to process binary files
    let get_binary_info = |buf: &[u8]| -> DiffResult {
        let kind = infer::get(buf);
        let mime = kind
            .map(|k| k.mime_type().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let ext = kind
            .map(|k| k.extension().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        DiffResult::Binary { size, mime, ext }
    };

    // 4. Git's 8KB Binary Check heuristic
    let check_len = std::cmp::min(buffer.len(), 8000);
    if buffer[..check_len].contains(&0) {
        return Err(get_binary_info(&buffer));
    }

    // 5. Final UTF-8 validation
    match String::from_utf8(buffer) {
        Ok(text) => Ok(text),
        Err(e) => {
            // If UTF-8 fails, we can recover the original buffer bytes and infer them!
            let original_buffer = e.into_bytes();
            Err(get_binary_info(&original_buffer))
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
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };

        let right_fut = async {
            if let Some(p) = &req.right_path {
                load_text_safely(p).await
            } else {
                Ok(String::new())
            }
        };

        let (left_res, right_res) = tokio::join!(left_fut, right_fut);

        let diff_result = match (left_res, right_res) {
            (Err(e), _) | (_, Err(e)) => e,
            (Ok(left_text), Ok(right_text)) => {
                if left_text.is_empty() && right_text.is_empty() {
                    return FullDiffRes {
                        node_path: req.node_path,
                        result: DiffResult::Empty,
                        right_path: req.right_path,
                    };
                }

                let input = InternedInput::new(left_text.as_str(), right_text.as_str());
                let diff = Diff::compute(Algorithm::Histogram, &input);

                let mut hunks = Vec::new();
                for hunk in diff.hunks() {
                    hunks.push(Hunk {
                        before_lines: (hunk.before.start as usize)..(hunk.before.end as usize),
                        after_lines: (hunk.after.start as usize)..(hunk.after.end as usize),
                    });
                }

                DiffResult::Text(FileDiff {
                    old_text: Arc::from(left_text),
                    new_text: Arc::from(right_text),
                    hunks,
                    line_selections: Default::default(),
                })
            }
        };

        tracing::trace!("Full diff computation finished");
        FullDiffRes {
            node_path: req.node_path,
            result: diff_result,
            right_path: req.right_path,
        }
    }
}
