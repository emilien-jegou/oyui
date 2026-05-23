use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::WorkerTask;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Helper to check metadata and binary status on the thread pool
fn get_file_info(path: &Path) -> (bool, isize, Option<String>) {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return (false, 0, None),
    };

    let size = meta.len() as isize;

    // Treat > 1MB as binary
    if size > 1024 * 1024 {
        return (true, size, None);
    }

    let buffer = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return (false, size, None),
    };

    let check_len = std::cmp::min(buffer.len(), 8000);
    let is_binary = buffer[..check_len].contains(&0);

    let text = if !is_binary {
        String::from_utf8(buffer).ok()
    } else {
        None
    };

    (is_binary || text.is_none(), size, text)
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
                    let (l_bin, l_size, l_text) = get_file_info(&left_path);
                    let (r_bin, r_size, r_text) = get_file_info(&right_path);

                    let stats = if l_bin || r_bin {
                        DiffStats::Binary {
                            bytes: r_size - l_size,
                        }
                    } else {
                        let l_str = l_text.unwrap_or_default();
                        let r_str = r_text.unwrap_or_default();

                        let input = InternedInput::new(l_str.as_str(), r_str.as_str());
                        let diff = Diff::compute(Algorithm::Myers, &input);

                        DiffStats::Text {
                            insertions: diff.count_additions() as usize,
                            deletions: diff.count_removals() as usize,
                        }
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
