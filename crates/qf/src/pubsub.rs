use spin::Mutex;
use alloc::vec::Vec;
use crate::event::Signal;

/// Subscription bitmap table for publish‑subscribe.
/// For each signal (0..=max_signal) a 64‑bit bitmap stores the
/// priorities of AOs that have subscribed.
pub struct PubSubTable {
    subscriptions: Mutex<Vec<u64>>,
    max_signal: u16,
}

impl PubSubTable {
    /// Create a new table for `max_signal` inclusive.
    /// All bits are cleared (no subscriptions).
    pub fn new(max_signal: u16) -> Self {
        let size = (max_signal as usize) + 1;
        let mut subs = Vec::with_capacity(size);
        for _ in 0..size {
            subs.push(0);
        }
        Self { subscriptions: Mutex::new(subs), max_signal }
    }

    fn idx(&self, signal: Signal) -> usize {
        let idx = signal.0 as usize;
        assert!(idx <= self.max_signal as usize, "signal out of range");
        idx
    }

    /// Subscribe the AO with `priority` (0..63) to `signal`.
    pub fn subscribe(&self, signal: Signal, priority: u8) {
        let mask = 1u64 << priority;
        let idx = self.idx(signal);
        let mut subs = self.subscriptions.lock();
        subs[idx] |= mask;
    }

    /// Unsubscribe the AO with `priority` from `signal`.
    pub fn unsubscribe(&self, signal: Signal, priority: u8) {
        let mask = !(1u64 << priority);
        let idx = self.idx(signal);
        let mut subs = self.subscriptions.lock();
        subs[idx] &= mask;
    }

    /// Remove all subscriptions for the given `priority`.
    pub fn unsubscribe_all(&self, priority: u8) {
        let mask = !(1u64 << priority);
        let mut subs = self.subscriptions.lock();
        for val in subs.iter_mut() {
            *val &= mask;
        }
    }

    /// Return bitmap of subscribed priorities for `signal`.
    pub fn subscribers(&self, signal: Signal) -> u64 {
        let idx = self.idx(signal);
        let subs = self.subscriptions.lock();
        subs[idx]
    }
}
