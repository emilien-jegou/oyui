//! Concurrent path-keyed cache with in-flight dedup and generation invalidation.
//!
//! # Lock-order contract (module-wide)
//!
//! [`CacheMap`] methods take the scc entry lock while the closure passed to
//! [`CacheMap::update`] runs. To keep the ordering global and deadlock-free:
//!
//! 1. **Never hold the tree lock while calling a `CacheMap` method** — acquire
//!    the cache first, release it, then touch the tree, or clone out of the
//!    entry and act on the clone.
//! 2. **Never take another lock inside an `update` closure** beyond the entry
//!    lock the map already holds: closures run under a write lock, so any wait
//!    they perform is a wait-with-lock-held.
//!
//! The one sanctioned exception is documented at [`CacheMap::set`]: the state
//! mutex is taken *inside* the entry lock, in that order, nowhere else.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// In-flight paths paired with the instant of their claim.
#[derive(Default)]
struct CacheState {
    in_flight: HashMap<PathBuf, std::time::Instant>,
    generation: u64,
    /// Bumped on every accepted `set`/`clear`; observers poll it to notice
    /// asynchronous writes without holding a lock on the map.
    version: u64,
}

/// How long an in-flight claim may stay uncontested before another request
/// is allowed to take it over. A worker that dies between its
/// `mark_started` and its `set`/`cancel` would otherwise leave the path
/// permanently claimed, parking an open view on `Loading...` forever.
const CLAIM_STALENESS: std::time::Duration = std::time::Duration::from_secs(30);

/// A cache keyed by path, with duplicate-work suppression.
///
/// Values are stored as `Arc<T>` to avoid deep clones on read. Computation for a
/// path is claimed via `mark_started` and completed via `set`; `clear` bumps the
/// generation so claims and writes made beforehand are rejected instead of
/// clobbering the cleared cache.
#[derive(Clone)]
pub struct CacheMap<T> {
    inner: scc::HashMap<PathBuf, Arc<T>>,
    state: Arc<Mutex<CacheState>>,
}

impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            inner: scc::HashMap::new(),
            state: Arc::new(Mutex::new(CacheState::default())),
        }
    }
}

impl<T> CacheMap<T> {
    /// Drops all values; in-flight work claimed under an older generation is rejected.
    pub fn clear(&self) {
        // Lock order state -> scc: the wipe must be ordered after any `set`
        // that already validated its generation, or it would erase-and-lose.
        let mut state = self.state.lock();
        state.in_flight.clear();
        state.generation += 1;
        state.version += 1;
        self.inner.clear_sync();
    }

    /// Current invalidation generation, for work that never calls `mark_started`.
    pub fn generation(&self) -> u64 {
        self.state.lock().generation
    }

    /// Monotonic write counter, bumped whenever a value is accepted or cleared.
    pub fn version(&self) -> u64 {
        self.state.lock().version
    }

    /// Returns a clone of the cached value, or `None` if not present.
    pub fn get(&self, path: &PathBuf) -> Option<Arc<T>> {
        self.inner.read_sync(path, |_, v| v.clone())
    }

    /// Mutates the cached value in place when uniquely owned, cloning otherwise.
    /// Returns `None` if the path is not cached.
    ///
    /// Runs while the entry is write-locked: the closure must not call `CacheMap`
    /// methods. Callers must not hold the tree lock unless no worker listener can
    /// hold a `CacheMap` lock while waiting on that tree lock (see `set`).
    pub fn update<R>(&self, path: &PathBuf, f: impl FnOnce(&mut T) -> R) -> Option<R>
    where
        T: Clone,
    {
        self.inner.update_sync(path, |_, v| f(Arc::make_mut(v)))
    }

    /// Completes a claim made by `mark_started`, unless `generation` is stale.
    /// Returns whether the value was written.
    pub fn set(&self, path: PathBuf, value: Arc<T>, generation: u64) -> bool {
        let mut state = self.state.lock();
        if state.generation != generation {
            // The claim died with its generation: release the path either
            // way. Leaving it in flight would block every future claim and
            // park an open view on `Loading...` until the process restarts.
            state.in_flight.remove(&path);
            return false;
        }
        state.in_flight.remove(&path);
        // Lock order: state mutex -> scc entry. Never call this while holding the
        // tree lock: `update` closures take the tree lock under the scc entry.
        //
        // `upsert_sync` is required: `insert_sync` rejects existing keys, which
        // would silently drop updates (e.g. undo restoring a cached diff).
        let _ = self.inner.upsert_sync(path, value);
        state.version += 1;
        true
    }

    /// Claims `path` for computation; returns the generation for `set`/`cancel`,
    /// or `None` if a live claim is already active. A claim older than
    /// [`CLAIM_STALENESS`] is taken over: a worker that died between its
    /// `mark_started` and its `set`/`cancel` must not block the file forever.
    pub fn mark_started(&self, path: &Path) -> Option<u64> {
        let mut state = self.state.lock();
        if let Some(claimed_at) = state.in_flight.get(path) {
            if claimed_at.elapsed() <= CLAIM_STALENESS {
                return None;
            }
        }
        state
            .in_flight
            .insert(path.to_path_buf(), std::time::Instant::now());
        Some(state.generation)
    }

    /// Returns true if a path is currently being computed.
    pub fn is_in_flight(&self, path: &Path) -> bool {
        self.state.lock().in_flight.contains_key(path)
    }

    /// Releases a claim unless `generation` is stale. Returns whether released.
    pub fn cancel(&self, path: &Path, generation: u64) -> bool {
        let mut state = self.state.lock();
        if state.generation != generation {
            // Symmetric with `set`: the claim died with its generation, so
            // release the path; leaving it blocked would park open files on
            // `Loading...` until the process restarts.
            state.in_flight.remove(path);
            return false;
        }
        state.in_flight.remove(path);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_prevents_duplicate_work_and_set_releases_it() {
        let map: CacheMap<u32> = CacheMap::default();
        let path = PathBuf::from("p");

        let generation = map.mark_started(&path).expect("first claim succeeds");
        assert!(
            map.mark_started(&path).is_none(),
            "a second claim must be rejected while in flight"
        );
        assert!(map.is_in_flight(&path));

        assert!(map.set(path.clone(), Arc::new(1), generation));
        assert!(!map.is_in_flight(&path), "set must release the claim");
        assert_eq!(map.get(&path).map(|v| *v), Some(1));
    }

    /// A set/cancel arriving after the generation moved must release the
    /// in-flight mark: otherwise the path can never be re-claimed and an
    /// open view parks on `Loading...` until the process restarts.
    #[test]
    fn stale_completion_releases_the_claim() {
        let map: CacheMap<u32> = CacheMap::default();
        let path = PathBuf::from("p");

        // Claim under generation N, then the generation moves while the
        // task is in flight (a `clear`).
        let claimed = map.mark_started(&path).unwrap();
        map.clear();

        // The task completes late: its set is rejected AND releases the
        // path, so later claims can proceed.
        assert!(!map.set(path.clone(), Arc::new(7), claimed));
        let gen = map.mark_started(&path).expect("re-claim after stale set");
        assert!(map.set(path.clone(), Arc::new(7), gen));
        assert!(map.get(&path).is_some());

        // `cancel` behaves symmetrically.
        let claimed2 = map.mark_started(&path).unwrap();
        map.clear();
        assert!(!map.cancel(&path, claimed2));
        assert!(
            map.mark_started(&path).is_some(),
            "claim re-usable after a stale cancel"
        );
    }

    #[test]
    fn clear_rejects_stale_writes_and_claims() {
        let map: CacheMap<u32> = CacheMap::default();
        let path = PathBuf::from("p");

        let stale_generation = map.mark_started(&path).unwrap();
        map.clear();

        assert!(
            !map.set(path.clone(), Arc::new(99), stale_generation),
            "a write from before clear must be rejected"
        );
        assert!(
            map.get(&path).is_none(),
            "stale write must not survive the wipe"
        );
        assert!(
            !map.cancel(&path, stale_generation),
            "stale cancel must not touch newer state"
        );

        let generation = map.mark_started(&path).expect("claim works after clear");
        assert!(map.set(path.clone(), Arc::new(1), generation));
        assert_eq!(map.get(&path).map(|v| *v), Some(1));
    }

    #[test]
    fn update_mutates_cached_values() {
        let map: CacheMap<u32> = CacheMap::default();
        let path = PathBuf::from("p");

        assert_eq!(
            map.update(&path, |v| *v += 1),
            None,
            "update of a missing path yields None"
        );

        map.set(path.clone(), Arc::new(1), map.generation());
        assert_eq!(
            map.update(&path, |v| {
                *v += 41;
            }),
            Some(())
        );
        assert_eq!(map.get(&path).map(|v| *v), Some(42));
    }

    /// Regression: `set` previously used insert semantics and silently dropped
    /// writes for already-cached paths (breaking undo restore).
    #[test]
    fn set_replaces_an_existing_value() {
        let map: CacheMap<u32> = CacheMap::default();
        let path = PathBuf::from("p");

        assert!(map.set(path.clone(), Arc::new(1), map.generation()));
        assert!(map.set(path.clone(), Arc::new(2), map.generation()));
        assert_eq!(map.get(&path).map(|v| *v), Some(2));
    }

    /// The write counter must advance on accepted writes so views can detect
    /// asynchronous stats updates.
    #[test]
    fn version_advances_on_each_write() {
        let map: CacheMap<u32> = CacheMap::default();
        let start = map.version();
        map.set(PathBuf::from("p"), Arc::new(1), map.generation());
        assert!(map.version() > start);
    }
}
