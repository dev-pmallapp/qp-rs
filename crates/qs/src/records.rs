//! Canonical QS record identifiers shared across the workspace.
//!
//! These numeric ids match the QP/Spy trace protocol so that the standard QSpy
//! host tools can decode records emitted by qp-rs.

/// QEP (state machine) related record identifiers.
pub mod qep {
    /// State entry action executed.
    pub const STATE_ENTRY: u8 = 1;
    /// State exit action executed.
    pub const STATE_EXIT:  u8 = 2;
    /// Nested initial transition taken.
    pub const STATE_INIT:  u8 = 3;
    /// Top-most initial transition taken.
    pub const INIT_TRAN:   u8 = 4;
    /// Internal transition (no state change).
    pub const INTERN_TRAN: u8 = 5;
    /// Regular state transition.
    pub const TRAN:        u8 = 6;
    /// Event ignored by the state machine.
    pub const IGNORED:     u8 = 7;
    /// Event dispatched to the state machine.
    pub const DISPATCH:    u8 = 8;
    /// Event reached the top state unhandled.
    pub const UNHANDLED:   u8 = 9;
    /// Transition to history pseudo-state (no timestamp, RTC step).
    pub const TRAN_HIST:   u8 = 55;
}

/// QF (framework) record identifiers.
pub mod qf {
    /// Active object deferred an event.
    pub const ACTIVE_DEFER:            u8 = 10;
    /// Active object recalled a deferred event.
    pub const ACTIVE_RECALL:           u8 = 11;
    /// Active object subscribed to a signal.
    pub const ACTIVE_SUBSCRIBE:        u8 = 12;
    /// Active object unsubscribed from a signal.
    pub const ACTIVE_UNSUBSCRIBE:      u8 = 13;
    /// Event posted (FIFO) to an active object.
    pub const ACTIVE_POST:             u8 = 14;
    /// Event posted (LIFO) to an active object.
    pub const ACTIVE_POST_LIFO:        u8 = 15;
    /// Event retrieved from an active object's queue.
    pub const ACTIVE_GET:              u8 = 16;
    /// Last event retrieved (queue became empty).
    pub const ACTIVE_GET_LAST:         u8 = 17;
    /// Recall attempted but no event was deferred.
    pub const ACTIVE_RECALL_ATTEMPT:   u8 = 18;
    /// Raw event queue initialized.
    pub const EQUEUE_INIT:             u8 = 19;
    /// Event posted (FIFO) to a raw event queue.
    pub const EQUEUE_POST:             u8 = 20;
    /// Event posted (LIFO) to a raw event queue.
    pub const EQUEUE_POST_LIFO:        u8 = 21;
    /// Event retrieved from a raw event queue.
    pub const EQUEUE_GET:              u8 = 22;
    /// Memory pool initialized.
    pub const MPOOL_INIT:              u8 = 23;
    /// Block obtained from a memory pool.
    pub const MPOOL_GET:               u8 = 24;
    /// Block returned to a memory pool.
    pub const MPOOL_PUT:               u8 = 25;
    /// Event published to subscribers.
    pub const PUBLISH:                 u8 = 26;
    /// New reference taken to an event.
    pub const NEW_REF:                 u8 = 27;
    /// New event allocated.
    pub const NEW:                     u8 = 28;
    /// Garbage-collect attempted (refcount not yet zero).
    pub const GC_ATTEMPT:              u8 = 29;
    /// Event garbage-collected (freed).
    pub const GC:                      u8 = 30;
    /// System clock tick processed.
    pub const TICK:                    u8 = 31;

    /// Time-event record identifiers (32–37).
    pub mod time_evt {
        /// Time event armed.
        pub const ARM:                 u8 = 32;
        /// One-shot time event auto-disarmed on expiry.
        pub const AUTO_DISARM:         u8 = 33;
        /// Disarm attempted on an already-disarmed event.
        pub const DISARM_ATTEMPT:      u8 = 34;
        /// Time event disarmed.
        pub const DISARM:              u8 = 35;
        /// Time event counter updated via `rearm()`.
        pub const REARM:               u8 = 36;
        /// Time event posted to its target active object.
        pub const POST:                u8 = 37;
    }

    /// Reference to an event deleted.
    pub const DELETE_REF:              u8 = 38;
    /// Critical section entered.
    pub const CRIT_ENTRY:              u8 = 39;
    /// Critical section exited.
    pub const CRIT_EXIT:               u8 = 40;
    /// Interrupt service routine entered.
    pub const ISR_ENTRY:              u8 = 41;
    /// Interrupt service routine exited.
    pub const ISR_EXIT:                u8 = 42;

    /// Post (FIFO) attempt failed (queue full / margin not met).
    pub const ACTIVE_POST_ATTEMPT:     u8 = 45;
    /// Raw event-queue post attempt failed.
    pub const EQUEUE_POST_ATTEMPT:     u8 = 46;
    /// Memory-pool get attempt failed (pool exhausted).
    pub const MPOOL_GET_ATTEMPT:       u8 = 47;
    /// Defer attempt failed (added in QP/C++ v8.0.4).
    pub const ACTIVE_DEFER_ATTEMPT:    u8 = 81;
}

/// Scheduler related record identifiers (50–53).
pub mod sched {
    /// Scheduler locked at a priority ceiling.
    pub const LOCK:   u8 = 50;
    /// Scheduler unlocked.
    pub const UNLOCK: u8 = 51;
    /// Scheduler selected the next task to run.
    pub const NEXT:   u8 = 52;
    /// Scheduler went idle.
    pub const IDLE:   u8 = 53;
}

/// Test/infrastructure record identifiers (58–70).
pub mod infra {
    /// QUTest run paused, awaiting host.
    pub const TEST_PAUSED: u8 = 58;
    /// QUTest probe value requested.
    pub const TEST_PROBE:  u8 = 59;
    /// Host-tool back-channel: target acknowledges a command.
    pub const TARGET_DONE: u8 = 65;
    /// Host-tool back-channel: status of last RX command.
    pub const RX_STATUS:   u8 = 66;
    /// Response to a host query for object data.
    pub const QUERY_DATA:  u8 = 67;
    /// Response to a host memory-peek request.
    pub const PEEK_DATA:   u8 = 68;
    /// Assertion failed on the target.
    pub const ASSERT_FAIL: u8 = 69;
    /// Framework `run()` loop entered.
    pub const QF_RUN:      u8 = 70;
}

/// QXK extended-kernel record identifiers (71–80).
pub mod qxk {
    /// Semaphore taken (count was > 0).
    pub const SEM_TAKE:           u8 = 71;
    /// Semaphore wait blocked (count == 0, thread suspended).
    pub const SEM_BLOCK:          u8 = 72;
    /// Semaphore signalled (count incremented, waiter possibly woken).
    pub const SEM_SIGNAL:         u8 = 73;
    /// Semaphore `try_wait` attempt failed (non-blocking path).
    pub const SEM_BLOCK_ATTEMPT:  u8 = 74;
    /// Mutex locked by calling thread.
    pub const MTX_LOCK:           u8 = 75;
    /// Mutex lock blocked (already held, thread suspended).
    pub const MTX_BLOCK:          u8 = 76;
    /// Mutex unlocked by calling thread.
    pub const MTX_UNLOCK:         u8 = 77;
    /// Mutex `try_lock` attempt failed (already held).
    pub const MTX_LOCK_ATTEMPT:   u8 = 78;
    /// Mutex `try_lock` attempt failed (non-blocking path, already held).
    pub const MTX_BLOCK_ATTEMPT:  u8 = 79;
    /// Mutex `unlock` attempt failed (caller is not the owner).
    pub const MTX_UNLOCK_ATTEMPT: u8 = 80;
}
