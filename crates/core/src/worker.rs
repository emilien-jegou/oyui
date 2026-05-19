use crossbeam_channel::{Receiver, Sender};
use imara_diff::{Algorithm, Diff, InternedInput};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use crate::diff::{FileDiff, Hunk};
use crate::diff_cache::DiffStats;
use crate::syntax::SyntaxEngine;

#[derive(Debug, Clone)]
pub enum WorkerRequest {
    ComputeStats {
        node_path: PathBuf,
        left_path: PathBuf,
        right_path: PathBuf,
    },
    ComputeFullDiff {
        node_path: PathBuf,
        left_path: PathBuf,
        right_path: PathBuf,
    },
    Shutdown,
}

pub enum AsyncWorkerEvent {
    DiffStatsReady(PathBuf, DiffStats),
    FullDiffReady(PathBuf, FileDiff),
}

pub fn spawn_worker(
    req_rx: Receiver<WorkerRequest>,
    ev_tx: Sender<AsyncWorkerEvent>,
    syntax_engine: Arc<SyntaxEngine>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(request) = req_rx.recv() {
            match request {
                WorkerRequest::ComputeStats {
                    node_path,
                    left_path,
                    right_path,
                } => {
                    let (left_res, right_res) = tokio::join!(
                        fs::read_to_string(&left_path),
                        fs::read_to_string(&right_path)
                    );
                    let left_text = left_res.unwrap_or_default();
                    let right_text = right_res.unwrap_or_default();

                    let input = InternedInput::new(left_text.as_str(), right_text.as_str());
                    let diff = Diff::compute(Algorithm::Histogram, &input);

                    let stats = DiffStats {
                        insertions: diff.count_additions() as usize,
                        deletions: diff.count_removals() as usize,
                    };

                    let _ = ev_tx.send(AsyncWorkerEvent::DiffStatsReady(node_path, stats));
                }

                WorkerRequest::ComputeFullDiff {
                    node_path,
                    left_path,
                    right_path,
                } => {
                    let (left_res, right_res) = tokio::join!(
                        fs::read_to_string(&left_path),
                        fs::read_to_string(&right_path)
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

                    // --- Syntax Highlighting Block ---
                    let syntax_set = &syntax_engine.syntax_set;
                    let theme = &syntax_engine.theme;
                    let syntax = syntax_set
                        .find_syntax_by_extension(
                            right_path
                                .extension()
                                .and_then(|s| s.to_str())
                                .unwrap_or(""),
                        )
                        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

                    let mut highlighter = syntect::easy::HighlightLines::new(syntax, theme);
                    let highlighted: Vec<Vec<_>> = right_text
                        .lines()
                        .map(|line| {
                            highlighter
                                .highlight_line(line, syntax_set)
                                .unwrap_or_default()
                                .into_iter()
                                .map(|(style, text)| (style, text.to_string()))
                                .collect()
                        })
                        .collect();

                    let file_diff = FileDiff {
                        old_text: Arc::from(left_text),
                        new_text: Arc::from(right_text),
                        hunks,
                        highlighted_new: highlighted,
                        line_selections: Default::default(),
                    };

                    let _ = ev_tx.send(AsyncWorkerEvent::FullDiffReady(node_path, file_diff));
                }

                WorkerRequest::Shutdown => {
                    break;
                }
            }
        }
    })
}
