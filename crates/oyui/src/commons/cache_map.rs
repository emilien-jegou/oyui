//! Concurrent path-keyed cache with in-flight dedup and generation invalidation.

use parking_lot::Mutex;
use scc::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

/// In-flight paths paired with the invalidation generation they were claimed under.
#[derive(Default)]
struct CacheState {
    in_flight: HashSet<PathBuf>,
    generation: u64,
    /// Bumped on every accepted `set`/`clear`; observers poll it to notice
    /// asynchronous writes without holding a lock on the map.
    version: u64,
}

/// A cache keyed by path, with duplicate-work suppression.
///
/// Values are stored as `Arc<T>` to avoid deep clones on read. Computation for a
/// path is claimed via `mark_started` and completed via `set`; `clear` bumps the
/// generation so claims and writes made beforehand are rejected instead of
/// clobbering the cleared cache.
#[derive(Clone)]
pub struct CacheMap<T> {
    inner: HashMap<PathBuf, Arc<T>>,
    state: Arc<Mutex<CacheState>>,
}

impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
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
            return false;
        }
        state.in_flight.remove(&path);
        // Lock order: state mutex -> scc entry. Never call this while holding the
        // tree lock: `update` closures take the tree lock under the scc entry.
        let _ = self.inner.insert_sync(path, value);
        state.version += 1;
        true
    }

    /// Claims `path` for computation; returns the generation for `set`/`cancel`,
    /// or `None` if a claim is already active.
    pub fn mark_started(&self, path: &PathBuf) -> Option<u64> {
        let mut state = self.state.lock();
        if !state.in_flight.insert(path.clone()) {
            return None;
        }
        Some(state.generation)
    }

    /// Returns true if a path is currently being computed.
    pub fn is_in_flight(&self, path: &PathBuf) -> bool {
        self.state.lock().in_flight.contains(path)
    }

    /// Releases a claim unless `generation` is stale. Returns whether released.
    pub fn cancel(&self, path: &PathBuf, generation: u64) -> bool {
        let mut state = self.state.lock();
        if state.generation != generation {
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
}
