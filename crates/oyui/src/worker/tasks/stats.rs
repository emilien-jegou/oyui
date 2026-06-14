use crate::diff::DiffStats;
use crate::diff_cache::DiffCache;
use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::{Listener, TaskerContext};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Stats;

#[derive(Debug, Clone)]
pub struct StatsReq {
    pub files: Vec<(PathBuf, PathBuf, PathBuf)>,
}

#[derive(Debug, Clone)]
pub struct StatsRes {
    pub stats: Vec<(PathBuf, DiffStats)>,
}

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

impl Listener<StatsReq, crate::worker::EventSender> for Stats {
    type Context = ();

    #[tracing::instrument(skip_all)]
    async fn handle(
        event: StatsReq,
        _ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Computing diff stats for {} files", event.files.len());

        let stats = tokio::task::spawn_blocking(move || {
            event
                .files
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

        tx.send(StatsRes { stats })?;
        Ok(())
    }
}

#[derive(TaskerContext)]
pub struct StatsResCtx {
    pub cache: DiffCache,
}

pub struct StatsResListener;
impl Listener<StatsRes, crate::worker::EventSender> for StatsResListener {
    type Context = StatsResCtx;

    async fn handle(
        event: StatsRes,
        ctx: Self::Context,
        _tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Applied Stats cache");
        for (node_path, stats) in event.stats {
            ctx.cache.stats.set(node_path, stats);
        }
        Ok(())
    }
}
