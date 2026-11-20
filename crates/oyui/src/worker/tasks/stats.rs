//! Batch diff-stat computation listener and its cache-writing consumer.
use crate::cli::DiffAlgorithm;
use crate::diff::DiffResult;
use crate::diff::DiffStats;
use crate::diff_cache::DiffCache;
use crate::worker::tasks::full_diff::load_file_content_safely;
use imara_diff::{Algorithm, Diff, InternedInput};
use oyui_tasker::{Listener, TaskerContext};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
pub struct Stats;

#[derive(Clone)]
pub struct StatsReq {
    pub files: Vec<(PathBuf, PathBuf, PathBuf)>,
}

#[derive(Clone)]
pub struct StatsRes {
    pub stats: Vec<(PathBuf, DiffStats)>,
}

/// Shared classification for the stats pass.
///
/// Delegates to [`load_file_content_safely`] so both passes apply the same
/// size limit and binary detection:
/// - `Ok(text)` → decodable text, not binary;
/// - `Err(Binary { .. } | DiffResult::TooLarge(..))` → binary, size known;
/// - `Err(DiffResult::Error(_))` → unreadable, treated as an empty side.
fn get_file_info(path: &Path) -> (bool, isize, Option<String>) {
    match load_file_content_safely(path) {
        Ok(text) => (false, text.len() as isize, Some(text)),
        Err(DiffResult::Binary { size, .. } | DiffResult::TooLarge(size)) => {
            (true, size as isize, None)
        }
        Err(_) => (false, 0, None),
    }
}

/// Worker context slice for the stats job.
///
/// Holds the worker's bounded compute pool so the batch never falls back to
/// rayon's global pool, which would take every core while the UI thread needs
/// one.
#[derive(TaskerContext)]
pub struct StatsCtx {
    pub cpu: Arc<rayon::ThreadPool>,
    pub algorithm: DiffAlgorithm,
}

impl Listener<StatsReq> for Stats {
    type Sender = crate::worker::EventSender;
    type Context = StatsCtx;

    #[tracing::instrument(skip_all)]
    async fn handle(
        event: StatsReq,
        ctx: Self::Context,
        tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Computing diff stats for {} files", event.files.len());

        let pool = ctx.cpu.clone();
        let stats = tokio::task::spawn_blocking(move || {
            pool.install(|| {
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
                            let diff = Diff::compute(ctx.algorithm.as_imara(), &input);

                            DiffStats::Text {
                                insertions: diff.count_additions() as usize,
                                deletions: diff.count_removals() as usize,
                            }
                        };

                        (node_path, stats)
                    })
                    .collect::<Vec<_>>()
            })
        })
        .await
        // A panic here used to take the listener task down; returning it lets
        // the mirror report the failure to the UI instead.
        .map_err(|join_err| eyre::eyre!("stats computation panicked: {join_err}"))?;

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
impl Listener<StatsRes> for StatsResListener {
    type Sender = crate::worker::EventSender;
    type Context = StatsResCtx;

    async fn handle(
        event: StatsRes,
        ctx: Self::Context,
        _tx: crate::worker::EventSender,
    ) -> eyre::Result<()> {
        tracing::debug!("Applied Stats cache");
        // Stats are never cleared mid-flight; pin the current generation.
        let generation = ctx.cache.stats.generation();
        for (node_path, stats) in event.stats {
            ctx.cache.stats.set(node_path, Arc::new(stats), generation);
        }
        Ok(())
    }
}
