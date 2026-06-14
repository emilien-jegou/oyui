use crate::commons::lazy::{CacheVersion, Lazy};
use scc::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone)]
pub struct CacheMap<T> {
    inner: HashMap<PathBuf, Lazy<T>>,
    generation: std::sync::Arc<AtomicUsize>,
}

impl<T> Default for CacheMap<T> {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
            generation: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl<T: Clone> CacheMap<T> {
    pub fn clear(&self) {
        self.inner.clear_sync();
    }

    pub fn get(&self, path: &PathBuf) -> Lazy<T> {
        let res = self.inner.read_sync(path, |_, v| Lazy::<T>::clone(v));
        res.unwrap_or(Lazy::Uninitialized)
    }

    /// Retrieves the current global invalidation generation.
    pub fn current_generation(&self) -> usize {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn set(&self, path: PathBuf, value: T) {
        let version = CacheVersion {
            generation: self.current_generation(),
            file_generation: 1,
        };
        self.set_versioned(path, version, value);
    }

    /// Atomically updates the cache value if the target version is up-to-date.
    /// Returns `true` if the write succeeded, or `false` if it was rejected as stale.
    pub fn set_versioned(&self, path: PathBuf, target_version: CacheVersion, value: T) -> bool {
        if target_version.generation < self.current_generation() {
            return false;
        }

        match self.inner.entry_sync(path) {
            scc::hash_map::Entry::Occupied(mut o) => {
                let state = o.get_mut();
                let should_write = match state {
                    Lazy::Uninitialized => true,
                    Lazy::Started(current_version)
                    | Lazy::Ready(current_version, _)
                    | Lazy::Stale(current_version, _)
                    | Lazy::StaleRestarted(current_version, _) => {
                        if target_version.generation < current_version.generation {
                            false
                        } else if target_version.generation
                            > current_version.generation
                        {
                            true
                        } else {
                            // Same global generation: only write if the file-level generation matches exactly
                            current_version.file_generation == target_version.file_generation
                        }
                    }
                };

                if should_write {
                    *state = Lazy::Ready(target_version, value);
                    true
                } else {
                    false
                }
            }
            scc::hash_map::Entry::Vacant(v) => {
                v.insert_entry(Lazy::Ready(target_version, value));
                true
            }
        }
    }

    /// Increments the global generation counter and invalidates active entries.
    pub fn invalidate_all(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let _ = self.inner.iter_mut_sync(|e| {
            let (_k, mut v) = e.consume();
            v.invalidate();
            true
        });
    }

    pub fn invalidate(&self, path: &PathBuf) {
        self.inner.update_sync(path, |_, v| {
            v.invalidate();
        });
    }

    /// Attempts to start a task for a given path.
    /// Returns `Some(CacheVersion)` if the task needs running, or `None` if it can be skipped.
    pub fn mark_started(&self, path: PathBuf, force_new_file_gen: bool) -> Option<CacheVersion> {
        let global_gen = self.current_generation();

        match self.inner.entry_sync(path) {
            scc::hash_map::Entry::Occupied(mut o) => {
                o.get_mut().start(global_gen, force_new_file_gen)
            }
            scc::hash_map::Entry::Vacant(v) => {
                let ver = CacheVersion {
                    generation: global_gen,
                    file_generation: 1,
                };
                v.insert_entry(Lazy::Started(ver.clone()));
                Some(ver)
            }
        }
    }

    pub fn inner(&self) -> &HashMap<PathBuf, Lazy<T>> {
        &self.inner
    }
}
