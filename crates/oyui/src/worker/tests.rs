//! Startup pipeline tests: events flow from the registry to shared state.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;

use crate::cli::DiffAlgorithm;
use crate::diff_cache::DiffCache;
use crate::syntax::SyntaxEngine;
use crate::terminal_colors::TerminalColorMode;
use crate::theme::ThemeState;
use crate::tree::FileTree;
use crate::worker::context::AppWorkerContext;
use crate::worker::tasks::calculate_file_tree::CalculateFileTreeReq;
use crate::worker::EventRegistry;

/// Polls `condition` until it yields `Some` or the timeout elapses.
async fn wait_for<T>(timeout: Duration, mut condition: impl FnMut() -> Option<T>) -> Option<T> {
    let start = std::time::Instant::now();
    loop {
        if let Some(value) = condition() {
            return Some(value);
        }
        if start.elapsed() > timeout {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// End-to-end startup: CalculateFileTreeReq must reach its listener, install
/// the tree, and trigger the downstream stats chain.
#[tokio::test]
async fn startup_pipeline_delivers_tree_and_stats() {
    let root = std::env::temp_dir().join(format!("oyui-pipeline-{}", std::process::id()));
    let left = root.join("left");
    let right = root.join("right");
    std::fs::create_dir_all(left.join("sub")).unwrap();
    std::fs::create_dir_all(right.join("sub")).unwrap();
    std::fs::write(left.join("sub/a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(right.join("sub/a.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(left.join("gone.txt"), "x\n").unwrap();
    std::fs::write(right.join("new.txt"), "y\n").unwrap();

    let tree = Arc::new(RwLock::new(FileTree::default()));
    let cache = DiffCache::default();
    let color_mode = TerminalColorMode::NoColor;

    let context = AppWorkerContext::builder()
        .syntax_engine(SyntaxEngine::new())
        .algorithm(DiffAlgorithm::Myers)
        .tree(tree.clone())
        .cache(cache.clone())
        .config_error(Arc::new(RwLock::new(None)))
        .theme(Arc::new(RwLock::new(ThemeState::new(&color_mode))))
        .build();

    let registry = EventRegistry::spawn(context);
    registry
        .send(CalculateFileTreeReq {
            left: left.clone(),
            right: right.clone(),
        })
        .expect("startup request queued");

    let file_count = wait_for(Duration::from_secs(10), || {
        let count = tree.read().files().count();
        (count >= 3).then_some(count)
    })
    .await;
    assert!(
        file_count.is_some(),
        "tree was never populated: the startup event did not reach its listener"
    );

    let stats_applied = wait_for(Duration::from_secs(10), || {
        cache
            .stats
            .get(&PathBuf::from("sub").join("a.txt"))
            .map(|_| ())
    })
    .await;
    assert!(
        stats_applied.is_some(),
        "StatsRes never reached the cache: the downstream event chain is broken"
    );

    registry
        .shutdown()
        .await
        .expect("registry shuts down cleanly");
    let _ = std::fs::remove_dir_all(&root);
}
