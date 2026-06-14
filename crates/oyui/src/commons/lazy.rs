//! This module defines the versioned `Lazy` type for safe async cache state tracking
//! using a two-level generation model.

/// Tracks cache states across both global changes (themes) and local file edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheVersion {
    /// Represents the global invalidation counter (e.g. incremented on theme changes).
    pub generation: usize,
    /// Represents the local file edit counter (incremented on each file modification).
    pub file_generation: usize,
}

/// Tracks the asynchronous initialization state of a cached value with optimistic concurrency control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lazy<T> {
    Uninitialized,
    Started(CacheVersion),
    Ready(CacheVersion, T),
    Stale(CacheVersion, T),
    StaleRestarted(CacheVersion, T),
}

impl<T> Default for Lazy<T> {
    fn default() -> Self {
        Lazy::Uninitialized
    }
}

impl<T> Lazy<T> {
    /// Attempts to transition the state to "started" or "restarted" for the target global generation.
    /// If `force_new_file_gen` is true, it increments the file-level generation.
    /// Returns `Some(CacheVersion)` if a new task should be run, or `None` if it is already up-to-date.
    pub fn start(&mut self, current_global_gen: usize, force_new_file_gen: bool) -> Option<CacheVersion> {
        let current_version = match self {
            Lazy::Uninitialized => None,
            Lazy::Started(ver)
            | Lazy::Ready(ver, _)
            | Lazy::Stale(ver, _)
            | Lazy::StaleRestarted(ver, _) => Some(ver.clone()),
        };

        let next_version = match current_version {
            None => CacheVersion {
                generation: current_global_gen,
                file_generation: 1,
            },
            Some(mut ver) => {
                let mut changed = false;
                if ver.generation != current_global_gen {
                    ver.generation = current_global_gen;
                    changed = true;
                }
                if force_new_file_gen {
                    ver.file_generation += 1;
                    changed = true;
                }

                let state_needs_run = matches!(self, Lazy::Uninitialized | Lazy::Stale(_, _));
                if !changed && !state_needs_run {
                    return None;
                }
                ver
            }
        };

        match self {
            Lazy::Uninitialized | Lazy::Started(_) => {
                *self = Lazy::Started(next_version.clone());
            }
            Lazy::Ready(_, _) | Lazy::Stale(_, _) | Lazy::StaleRestarted(_, _) => {
                let old = std::mem::replace(self, Lazy::Uninitialized);
                if let Lazy::Ready(_, v) | Lazy::Stale(_, v) | Lazy::StaleRestarted(_, v) = old {
                    *self = Lazy::StaleRestarted(next_version.clone(), v);
                }
            }
        }

        Some(next_version)
    }

    /// Transitions a ready/stale/restarted state into `Stale` (invalidated),
    /// keeping the current version and underlying value.
    pub fn invalidate(&mut self) {
        let prev = std::mem::replace(self, Lazy::Uninitialized);
        match prev {
            Lazy::Ready(ver, v) | Lazy::Stale(ver, v) | Lazy::StaleRestarted(ver, v) => {
                *self = Lazy::Stale(ver, v);
            }
            Lazy::Started(ver) => {
                *self = Lazy::Started(ver);
            }
            _ => *self = Lazy::Uninitialized,
        }
    }

    /// Access the underlying value if it exists, even if stale.
    pub fn value(&self) -> Option<&T> {
        match self {
            Lazy::Ready(_, v) | Lazy::Stale(_, v) | Lazy::StaleRestarted(_, v) => Some(v),
            _ => None,
        }
    }

    /// Access the version if it has been initialized.
    pub fn version(&self) -> Option<&CacheVersion> {
        match self {
            Lazy::Started(ver)
            | Lazy::Ready(ver, _)
            | Lazy::Stale(ver, _)
            | Lazy::StaleRestarted(ver, _) => Some(ver),
            _ => None,
        }
    }
}
