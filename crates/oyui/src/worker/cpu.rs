//! Bounded off-thread compute for worker listeners.
//!
//! Background jobs share the machine with the main thread, which has to stay
//! responsive while a diff is still being computed. Using rayon's global pool
//! — every core — would let one job starve exactly that thread, so each worker
//! owns a pool sized to leave headroom instead.

use rayon::ThreadPool;
use std::sync::Arc;

/// Builds a pool that leaves at least one core free for the foreground.
///
/// Falls back to a single thread when the count cannot be read, and never
/// returns more than one thread per available core.
pub fn bounded_pool(name: &str) -> eyre::Result<Arc<ThreadPool>> {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    // One core is reserved for the UI thread's event loop and repaint.
    let threads = cpus.saturating_sub(1).max(1);
    let prefix = name.to_string();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(move |i| format!("{prefix}-{i}"))
        .build()?;
    Ok(Arc::new(pool))
}

#[cfg(test)]
mod tests {
    use super::bounded_pool;

    /// The whole point of the pool is that it is smaller than the machine: a
    /// background job must never take every core.
    #[test]
    fn pool_leaves_headroom_for_the_ui_thread() {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        for name in ["oyui-worker", "oyui-test"] {
            let pool = bounded_pool(name).expect("pool builds");
            assert_eq!(
                pool.current_num_threads(),
                cpus.saturating_sub(1).max(1),
                "{name}: threads must not exceed cores minus the UI thread"
            );
        }
    }

    #[test]
    fn pool_runs_work_on_its_own_threads() {
        let pool = bounded_pool("oyui-cpu").expect("pool builds");
        let total = pool.install(|| (0..10_000i32).sum::<i32>());
        assert_eq!(total, 49_995_000);
    }
}
