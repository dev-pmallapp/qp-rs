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

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use crate::active::ActiveRunnable;
use crate::event::DynEvent;
use crate::sync::Mutex;

// ── QEQueue ───────────────────────────────────────────────────────────────────

struct QEQueueInner {
    buffer: VecDeque<DynEvent>,
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
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(QEQueueInner {
                buffer: VecDeque::with_capacity(capacity),
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
            inner.buffer.push_back(event);
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
            inner.buffer.push_front(event);
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
