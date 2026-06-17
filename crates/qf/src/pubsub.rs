use alloc::vec;
use alloc::vec::Vec;
use crate::event::Signal;
use crate::sync::Mutex;

/// Subscription bitmap table for publish‑subscribe.
/// For each signal (0..=max_signal) a 64‑bit bitmap stores the
/// priorities of AOs that have subscribed.
///
/// The bitmaps are held behind a single [`Mutex`] rather than per-entry
/// atomics so the table is portable to targets without native 64-bit atomics
/// (e.g. `riscv32imac` / `thumbv6m`); subscribe/unsubscribe/publish are not hot
/// paths, so the lock cost is negligible.
pub struct PubSubTable {
    subscriptions: Mutex<Vec<u64>>,
    max_signal: u16,
}

impl PubSubTable {
    /// Create a new table for `max_signal` inclusive.
    /// All bits are cleared (no subscriptions).
    pub fn new(max_signal: u16) -> Self {
        let size = (max_signal as usize) + 1;
        Self {
            subscriptions: Mutex::new(vec![0u64; size]),
            max_signal,
        }
    }

    fn idx(&self, signal: Signal) -> usize {
        let idx = signal.0 as usize;
        assert!(idx <= self.max_signal as usize, "signal out of range");
        idx
    }

    /// Subscribe the AO with `priority` (0..63) to `signal`.
    pub fn subscribe(&self, signal: Signal, priority: u8) {
        let idx = self.idx(signal);
        self.subscriptions.lock()[idx] |= 1u64 << priority;
    }

    /// Unsubscribe the AO with `priority` from `signal`.
    pub fn unsubscribe(&self, signal: Signal, priority: u8) {
        let idx = self.idx(signal);
        self.subscriptions.lock()[idx] &= !(1u64 << priority);
    }

    /// Remove all subscriptions for the given `priority`.
    pub fn unsubscribe_all(&self, priority: u8) {
        let mask = !(1u64 << priority);
        for bits in self.subscriptions.lock().iter_mut() {
            *bits &= mask;
        }
    }

    /// Return bitmap of subscribed priorities for `signal`.
    pub fn subscribers(&self, signal: Signal) -> u64 {
        let idx = self.idx(signal);
        self.subscriptions.lock()[idx]
    }
}
