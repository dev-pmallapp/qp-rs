//! Standalone raw event queue — QEQueue equivalent (Phase 4).
//!
//! `QEQueue` is a fixed-capacity ring buffer for deferring events, bridging
//! ISR→AO communication, and any other purpose that needs a standalone event
//! holding area outside an active object.
//!
//! Thread-safety is provided by an internal mutex.  All operations are O(1).
//!
//! # Watermark
//!
//! The queue tracks `min_free` — the minimum number of free slots ever observed.
//! This low-water mark is sticky: it never increases after being set, giving a
//! worst-case queue-fullness diagnostic without extra tooling.
//!
//! # Defer / Recall
//!
//! `defer()` and `recall()` implement the QP/C++ pattern for holding events
//! that arrive before the state machine is ready to handle them:
//!
//! ```rust,ignore
//! // Inside a state handler:
//! defer(&my_ao, &my_defer_q, event);          // park the event
//! // ...later, in a state that can handle it:
//! if recall(&my_ao, &my_defer_q) {             // re-injects via LIFO
//!     // event is now at the front of my_ao's queue
//! }
//! ```

#[cfg(all(not(feature = "std"), not(feature = "static-alloc")))]
use alloc::collections::VecDeque;
#[cfg(all(feature = "std", not(feature = "static-alloc")))]
use std::collections::VecDeque;

use crate::active::ActiveRunnable;
use crate::event::DynEvent;
use crate::sync::Mutex;

// ── QEQueue ───────────────────────────────────────────────────────────────────

/// Inline storage capacity for [`QEQueue`] under the `static-alloc` (heap-free)
/// build. The runtime `capacity` passed to [`QEQueue::new`] is the *logical*
/// limit and must not exceed this fixed inline bound.
#[cfg(feature = "static-alloc")]
pub const QEQUEUE_CAPACITY: usize = 16;

struct QEQueueInner {
    #[cfg(not(feature = "static-alloc"))]
    buffer: VecDeque<DynEvent>,
    #[cfg(feature = "static-alloc")]
    buffer: heapless::Deque<DynEvent, QEQUEUE_CAPACITY>,
    capacity: usize,
    min_free: usize,
}

impl QEQueueInner {
    #[inline]
    fn update_watermark(&mut self) {
        let free = self.capacity.saturating_sub(self.buffer.len());
        if free < self.min_free {
            self.min_free = free;
        }
    }
}

/// Fixed-capacity FIFO event queue with watermark tracking.
///
/// Corresponds to `QEQueue` in QP/C++.
pub struct QEQueue {
    inner: Mutex<QEQueueInner>,
}

impl QEQueue {
    /// Create a new queue that holds at most `capacity` events.
    ///
    /// Under `static-alloc`, `capacity` is the logical limit over fixed inline
    /// storage and must not exceed [`QEQUEUE_CAPACITY`]; a larger request is a
    /// configuration fault.
    pub fn new(capacity: usize) -> Self {
        #[cfg(feature = "static-alloc")]
        if capacity > QEQUEUE_CAPACITY {
            crate::fusa::on_error(module_path!(), line!());
        }
        Self {
            inner: Mutex::new(QEQueueInner {
                #[cfg(not(feature = "static-alloc"))]
                buffer: VecDeque::with_capacity(capacity),
                #[cfg(feature = "static-alloc")]
                buffer: heapless::Deque::new(),
                capacity,
                min_free: capacity,
            }),
        }
    }

    /// Post `event` FIFO. Returns `false` if `free_slots <= margin`.
    ///
    /// `margin = 0` succeeds whenever any slot is free.
    pub fn post(&self, event: DynEvent, margin: usize) -> bool {
        let mut inner = self.inner.lock();
        let free = inner.capacity.saturating_sub(inner.buffer.len());
        if free > margin {
            #[cfg(not(feature = "static-alloc"))]
            inner.buffer.push_back(event);
            // `free > margin >= 0` and `capacity <= QEQUEUE_CAPACITY`, so a free
            // inline slot is guaranteed — a push failure is an integrity fault.
            #[cfg(feature = "static-alloc")]
            if inner.buffer.push_back(event).is_err() {
                crate::fusa::on_error(module_path!(), line!());
            }
            inner.update_watermark();
            true
        } else {
            false
        }
    }

    /// Post `event` LIFO (to the front). Returns `false` if the queue is full.
    pub fn post_lifo(&self, event: DynEvent) -> bool {
        let mut inner = self.inner.lock();
        if inner.buffer.len() < inner.capacity {
            #[cfg(not(feature = "static-alloc"))]
            inner.buffer.push_front(event);
            #[cfg(feature = "static-alloc")]
            if inner.buffer.push_front(event).is_err() {
                crate::fusa::on_error(module_path!(), line!());
            }
            inner.update_watermark();
            true
        } else {
            false
        }
    }

    /// Remove and return the front event. Returns `None` if empty.
    pub fn get(&self) -> Option<DynEvent> {
        self.inner.lock().buffer.pop_front()
    }

    /// `true` if the queue holds at least one event.
    pub fn peek_front(&self) -> bool {
        !self.inner.lock().buffer.is_empty()
    }

    /// `true` if the queue currently holds no events.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().buffer.is_empty()
    }

    /// Number of free slots currently available.
    pub fn get_free(&self) -> usize {
        let inner = self.inner.lock();
        inner.capacity.saturating_sub(inner.buffer.len())
    }

    /// Minimum free slots ever observed (low-watermark, sticky).
    pub fn get_min(&self) -> usize {
        self.inner.lock().min_free
    }

    /// Number of events currently in the queue.
    pub fn len(&self) -> usize {
        self.inner.lock().buffer.len()
    }

    /// Maximum number of events the queue can hold.
    pub fn capacity(&self) -> usize {
        self.inner.lock().capacity
    }
}

// ── StaticEQueue (heap-free, `static-alloc` feature) ───────────────────────────

#[cfg(feature = "static-alloc")]
struct StaticEQueueInner<const N: usize> {
    buffer: heapless::Deque<DynEvent, N>,
    min_free: usize,
}

#[cfg(feature = "static-alloc")]
impl<const N: usize> StaticEQueueInner<N> {
    #[inline]
    fn update_watermark(&mut self) {
        let free = N.saturating_sub(self.buffer.len());
        if free < self.min_free {
            self.min_free = free;
        }
    }
}

/// Fixed-capacity, **heap-free** FIFO event queue with watermark tracking.
///
/// A drop-in analogue of [`QEQueue`] whose storage lives entirely inline — no
/// `VecDeque`, no heap — making it suitable for the `no_std` + `static-alloc`
/// functional-safety build (see `docs/FUSA.md`, Phase 2). Capacity is fixed at
/// the type level by the const generic `N`.
///
/// Because [`new`](Self::new) is `const`, an instance can live in `static`
/// storage with no runtime initialisation:
///
/// ```
/// # #[cfg(feature = "static-alloc")] {
/// use qf::equeue::StaticEQueue;
/// static DEFER_Q: StaticEQueue<8> = StaticEQueue::new();
/// # }
/// ```
#[cfg(feature = "static-alloc")]
pub struct StaticEQueue<const N: usize> {
    inner: Mutex<StaticEQueueInner<N>>,
}

#[cfg(feature = "static-alloc")]
impl<const N: usize> Default for StaticEQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "static-alloc")]
impl<const N: usize> StaticEQueue<N> {
    /// Create an empty queue with inline capacity `N`.
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(StaticEQueueInner {
                buffer: heapless::Deque::new(),
                min_free: N,
            }),
        }
    }

    /// Post `event` FIFO. Returns `false` if `free_slots <= margin`.
    ///
    /// `margin = 0` succeeds whenever any slot is free.
    pub fn post(&self, event: DynEvent, margin: usize) -> bool {
        let mut inner = self.inner.lock();
        let free = N.saturating_sub(inner.buffer.len());
        if free > margin {
            // `free > margin >= 0` guarantees a free slot, so `push_back`
            // cannot fail. A failure would mean the length/capacity accounting
            // is corrupt — a data-integrity invariant violation.
            if inner.buffer.push_back(event).is_err() {
                crate::fusa::on_error(module_path!(), line!());
            }
            inner.update_watermark();
            true
        } else {
            false
        }
    }

    /// Post `event` LIFO (to the front). Returns `false` if the queue is full.
    pub fn post_lifo(&self, event: DynEvent) -> bool {
        let mut inner = self.inner.lock();
        if inner.buffer.len() < N {
            if inner.buffer.push_front(event).is_err() {
                crate::fusa::on_error(module_path!(), line!());
            }
            inner.update_watermark();
            true
        } else {
            false
        }
    }

    /// Remove and return the front event. Returns `None` if empty.
    pub fn get(&self) -> Option<DynEvent> {
        self.inner.lock().buffer.pop_front()
    }

    /// `true` if the queue holds at least one event.
    pub fn peek_front(&self) -> bool {
        !self.inner.lock().buffer.is_empty()
    }

    /// `true` if the queue currently holds no events.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().buffer.is_empty()
    }

    /// Number of free slots currently available.
    pub fn get_free(&self) -> usize {
        let inner = self.inner.lock();
        N.saturating_sub(inner.buffer.len())
    }

    /// Minimum free slots ever observed (low-watermark, sticky).
    pub fn get_min(&self) -> usize {
        self.inner.lock().min_free
    }

    /// Number of events currently in the queue.
    pub fn len(&self) -> usize {
        self.inner.lock().buffer.len()
    }

    /// Maximum number of events the queue can hold (the const capacity `N`).
    pub const fn capacity(&self) -> usize {
        N
    }
}

// ── Defer / Recall ────────────────────────────────────────────────────────────

/// Defer `event` to queue `eq` on behalf of active object `ao`.
///
/// Returns `true` if the event was accepted (margin = 0, any free slot
/// suffices).  Returns `false` if the defer queue is full.
///
/// Corresponds to `QActive::defer()` in QP/C++.
pub fn defer(ao: &dyn ActiveRunnable, eq: &QEQueue, event: DynEvent) -> bool {
    let _ = ao;
    eq.post(event, 0)
}

/// Recall the front event from `eq` and re-inject it LIFO into `ao`'s queue.
///
/// Returns `true` if an event was moved.  The recalled event is placed at the
/// front of `ao`'s queue so it will be the next event dispatched.
///
/// Corresponds to `QActive::recall()` in QP/C++.
pub fn recall(ao: &dyn ActiveRunnable, eq: &QEQueue) -> bool {
    if let Some(event) = eq.get() {
        ao.post_lifo(event);
        true
    } else {
        false
    }
}

/// Discard up to `num` events from `eq`.
///
/// Pass `usize::MAX` to flush the entire queue.
/// Returns the number of events discarded.
///
/// Corresponds to `QActive::flushDeferred()` in QP/C++.
pub fn flush_deferred(eq: &QEQueue, num: usize) -> usize {
    let mut count = 0;
    while count < num {
        if eq.get().is_none() {
            break;
        }
        count += 1;
    }
    count
}

#[cfg(all(test, feature = "static-alloc"))]
mod static_tests {
    use super::StaticEQueue;
    use crate::event::{DynEvent, Signal};

    fn ev(sig: u16) -> DynEvent {
        DynEvent::empty_dyn(Signal(sig))
    }

    #[test]
    fn fifo_order_and_capacity() {
        let q: StaticEQueue<4> = StaticEQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.capacity(), 4);

        assert!(q.post(ev(1), 0));
        assert!(q.post(ev(2), 0));
        assert!(q.post(ev(3), 0));
        assert!(q.post(ev(4), 0));
        // Queue full — further posts are rejected, not faulted.
        assert!(!q.post(ev(5), 0));
        assert_eq!(q.len(), 4);
        assert_eq!(q.get_free(), 0);

        assert_eq!(q.get().unwrap().signal(), Signal(1));
        assert_eq!(q.get().unwrap().signal(), Signal(2));
        assert_eq!(q.get().unwrap().signal(), Signal(3));
        assert_eq!(q.get().unwrap().signal(), Signal(4));
        assert!(q.get().is_none());
    }

    #[test]
    fn margin_reserves_slots() {
        let q: StaticEQueue<3> = StaticEQueue::new();
        // margin = 1 keeps one slot free.
        assert!(q.post(ev(1), 1)); // free 3 > 1
        assert!(q.post(ev(2), 1)); // free 2 > 1
        assert!(!q.post(ev(3), 1)); // free 1 !> 1 → rejected
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn lifo_jumps_the_queue() {
        let q: StaticEQueue<3> = StaticEQueue::new();
        assert!(q.post(ev(1), 0));
        assert!(q.post_lifo(ev(99)));
        assert_eq!(q.get().unwrap().signal(), Signal(99));
        assert_eq!(q.get().unwrap().signal(), Signal(1));
    }

    #[test]
    fn min_free_watermark_is_sticky() {
        let q: StaticEQueue<4> = StaticEQueue::new();
        assert_eq!(q.get_min(), 4);
        q.post(ev(1), 0);
        q.post(ev(2), 0);
        q.post(ev(3), 0); // free now 1
        assert_eq!(q.get_min(), 1);
        q.get(); // drain — free rises, but min stays sticky
        q.get();
        assert_eq!(q.get_min(), 1);
    }
}
