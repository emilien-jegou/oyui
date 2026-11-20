//! A generation cell that collapses bursts of same-class events.
//!
//! Replaces queue surgery as the coalescing mechanism (finding 3.2): the
//! producer stores every incoming payload into a shared slot — newest wins —
//! and only a store that transitions the cell from empty to occupied signals
//! a wake-up downstream. The payload crosses the bus once per burst, the
//! producer never touches the consumer's queue, and memory is bounded by one
//! slot per collapsing class instead of one queue entry per event.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// A slot that keeps only the newest pending payload for one event class.
///
/// The producer stores every incoming payload; the slot keeps the newest and
/// a store that transitions the cell from empty (or fully consumed) to
/// occupied is the signal that a fresh wake-up is owed downstream. The
/// consumer drains with [`Self::take`], which also re-arms the cell so the
/// next burst signals again.
pub struct CoalesceCell<T> {
    slot: Mutex<Option<T>>,
    /// True while an unconsumed store is pending; the consumer's drain clears
    /// it so the next burst's first store is signalled again.
    pending: AtomicBool,
    /// Bumped once per accepted store; lets a consumer detect "nothing new
    /// since I last drained" without locking the slot.
    generation: AtomicU64,
}

impl<T> CoalesceCell<T> {
    /// Creates an empty cell.
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            pending: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    /// Stores `value` as the pending payload, replacing any older one.
    ///
    /// Returns `true` when this store transitioned the cell from empty (or
    /// fully consumed) to occupied — the signal that a wake-up is owed
    /// downstream. Later stores in the same burst overwrite the slot
    /// silently: the consumer only ever needs the newest payload.
    pub fn store(&self, value: T) -> bool {
        // Reserve the slot first so a concurrent drain cannot race the
        // write; the lock is held across the pending flip, making
        // store-and-signal one atomic step from the reader's perspective.
        let mut slot = self.slot.lock().unwrap();
        let was_pending = self.pending.swap(true, Ordering::AcqRel);
        *slot = Some(value);
        drop(slot);
        self.generation.fetch_add(1, Ordering::Relaxed);
        !was_pending
    }

    /// Drains the newest pending payload, leaving the cell empty.
    ///
    /// The pending flag is cleared while the slot lock is held. Both a store
    /// and a drain therefore run entirely under one lock acquisition, so a
    /// store racing a drain observes the flag state the drain left behind:
    /// a store that overwrites an un-drained payload correctly stays silent,
    /// and one that arrives after the drain re-signals. Clearing the flag
    /// after unlocking would open a window where a racing store sees the
    /// stale flag and skips a wake-up that is still owed.
    pub fn take(&self) -> Option<T> {
        let mut slot = self.slot.lock().unwrap();
        let value = slot.take();
        if value.is_some() {
            self.pending.store(false, Ordering::Release);
        }
        value
    }

    /// True while an unconsumed store is pending.
    pub fn is_pending(&self) -> bool {
        self.pending.load(Ordering::Acquire)
    }

    /// Bumped once per accepted store; a consumer that noted the generation
    /// before its last drain can skip the next drain when this is unchanged.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
}

impl<T> Default for CoalesceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_store_of_a_burst_signals() {
        let cell: CoalesceCell<u8> = CoalesceCell::new();
        assert!(
            cell.store(1),
            "the first store of a burst asks for a wake-up"
        );
        assert!(!cell.store(2), "later stores in the same burst are silent");
        assert!(!cell.store(3));
    }

    #[test]
    fn draining_the_cell_rearms_it() {
        let cell: CoalesceCell<&'static str> = CoalesceCell::new();
        cell.store("a");
        cell.store("b");
        assert_eq!(cell.take(), Some("b"));
        assert_eq!(cell.take(), None);
        assert!(
            cell.store("c"),
            "a drained cell signals the next burst again"
        );
    }

    #[test]
    fn generation_advances_per_store() {
        let cell: CoalesceCell<u8> = CoalesceCell::new();
        let before = cell.generation();
        cell.store(1);
        cell.store(2);
        assert!(cell.generation() >= before + 2);
    }

    #[test]
    fn stores_racing_a_drain_never_lose_the_pending_signal() {
        // Hammers the interleaving where a drain re-arms while a store is
        // overwriting: whatever the schedule, the invariant to hold is "at
        // most one payload pending, and the cell is re-armed once empty" —
        // otherwise a post-drain burst would never wake the consumer again.
        use std::sync::Arc;
        let cell = Arc::new(CoalesceCell::<u32>::new());
        let writer = {
            let cell = Arc::clone(&cell);
            std::thread::spawn(move || {
                for i in 0..10_000 {
                    cell.store(i);
                }
            })
        };
        while !writer.is_finished() {
            let _ = cell.take();
            std::thread::yield_now();
        }
        writer.join().unwrap();

        // At most one payload survives, and draining it re-arms the cell.
        if cell.take().is_some() {
            assert_eq!(cell.take(), None, "a cell holds a single payload");
        }
        assert!(
            cell.store(u32::MAX),
            "a fully drained cell signals the next burst"
        );
    }
}
