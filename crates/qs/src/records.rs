//! Canonical QS record identifiers shared across the workspace.

/// QEP (state machine) related record identifiers.
pub mod qep {
    pub const STATE_ENTRY: u8 = 1;
    pub const STATE_EXIT:  u8 = 2;
    pub const STATE_INIT:  u8 = 3;
    pub const INIT_TRAN:   u8 = 4;
    pub const INTERN_TRAN: u8 = 5;
    pub const TRAN:        u8 = 6;
    pub const IGNORED:     u8 = 7;
    pub const DISPATCH:    u8 = 8;
    pub const UNHANDLED:   u8 = 9;
    /// Transition to history pseudo-state (no timestamp, RTC step).
    pub const TRAN_HIST:   u8 = 55;
}

/// QF (framework) record identifiers.
pub mod qf {
    /// Active object records (10–18).
    pub const ACTIVE_DEFER:            u8 = 10;
    pub const ACTIVE_RECALL:           u8 = 11;
    pub const ACTIVE_SUBSCRIBE:        u8 = 12;
    pub const ACTIVE_UNSUBSCRIBE:      u8 = 13;
    pub const ACTIVE_POST:             u8 = 14;
    pub const ACTIVE_POST_LIFO:        u8 = 15;
    pub const ACTIVE_GET:              u8 = 16;
    pub const ACTIVE_GET_LAST:         u8 = 17;
    pub const ACTIVE_RECALL_ATTEMPT:   u8 = 18;
    /// Event-queue records (19–22).
    pub const EQUEUE_INIT:             u8 = 19;
    pub const EQUEUE_POST:             u8 = 20;
    pub const EQUEUE_POST_LIFO:        u8 = 21;
    pub const EQUEUE_GET:              u8 = 22;
    /// Memory-pool records (23–25).
    pub const MPOOL_INIT:              u8 = 23;
    pub const MPOOL_GET:               u8 = 24;
    pub const MPOOL_PUT:               u8 = 25;
    /// Event lifecycle records (26–31).
    pub const PUBLISH:                 u8 = 26;
    pub const NEW_REF:                 u8 = 27;
    pub const NEW:                     u8 = 28;
    pub const GC_ATTEMPT:              u8 = 29;
    pub const GC:                      u8 = 30;
    pub const TICK:                    u8 = 31;

    /// Time-event record identifiers (32–37).
    pub mod time_evt {
        pub const ARM:                 u8 = 32;
        pub const AUTO_DISARM:         u8 = 33;
        pub const DISARM_ATTEMPT:      u8 = 34;
        pub const DISARM:              u8 = 35;
        pub const REARM:               u8 = 36;
        pub const POST:                u8 = 37;
    }

    /// Framework/OS service records (38–42).
    pub const DELETE_REF:              u8 = 38;
    pub const CRIT_ENTRY:              u8 = 39;
    pub const CRIT_EXIT:               u8 = 40;
    pub const ISR_ENTRY:               u8 = 41;
    pub const ISR_EXIT:                u8 = 42;

    /// Post/get attempt failure records (45–47).
    pub const ACTIVE_POST_ATTEMPT:     u8 = 45;
    pub const EQUEUE_POST_ATTEMPT:     u8 = 46;
    pub const MPOOL_GET_ATTEMPT:       u8 = 47;
    /// Defer/recall attempt records (added in QP/C++ v8.0.4).
    pub const ACTIVE_DEFER_ATTEMPT:    u8 = 81;
}

/// Scheduler related record identifiers (50–53).
pub mod sched {
    pub const LOCK:   u8 = 50;
    pub const UNLOCK: u8 = 51;
    pub const NEXT:   u8 = 52;
    pub const IDLE:   u8 = 53;
}

/// Test/infrastructure record identifiers (58–70).
pub mod infra {
    pub const TEST_PAUSED: u8 = 58;
    pub const TEST_PROBE:  u8 = 59;
    pub const QUERY_DATA:  u8 = 67;
    pub const PEEK_DATA:   u8 = 68;
    pub const ASSERT_FAIL: u8 = 69;
    pub const QF_RUN:      u8 = 70;
}
