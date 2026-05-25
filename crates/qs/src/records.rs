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
}

/// QF (framework) record identifiers.
pub mod qf {
    /// Active object records (10–17).
    pub const ACTIVE_DEFER:       u8 = 10;
    pub const ACTIVE_RECALL:      u8 = 11;
    pub const ACTIVE_SUBSCRIBE:   u8 = 12;
    pub const ACTIVE_UNSUBSCRIBE: u8 = 13;
    pub const ACTIVE_POST:        u8 = 14;
    pub const ACTIVE_POST_LIFO:   u8 = 15;
    pub const ACTIVE_GET:         u8 = 16;
    pub const ACTIVE_GET_LAST:    u8 = 17;
    /// Event-queue records (18–22).
    pub const EQUEUE_INIT:        u8 = 18;
    pub const EQUEUE_POST:        u8 = 19;
    pub const EQUEUE_POST_LIFO:   u8 = 20;
    pub const EQUEUE_GET:         u8 = 21;
    pub const EQUEUE_GET_LAST:    u8 = 22;
    /// Memory-pool records (23–25).
    pub const MPOOL_INIT:         u8 = 23;
    pub const MPOOL_GET:          u8 = 24;
    pub const MPOOL_PUT:          u8 = 25;
    /// Event lifecycle records (26–31).
    pub const PUBLISH:            u8 = 26;
    pub const NEW_REF:            u8 = 27;
    pub const NEW:                u8 = 28;
    pub const GC_ATTEMPT:         u8 = 29;
    pub const GC:                 u8 = 30;
    pub const TICK:               u8 = 31;

    /// Time-event record identifiers (32–37).
    pub mod time_evt {
        pub const ARM:            u8 = 32;
        pub const AUTO_DISARM:    u8 = 33;
        pub const DISARM_ATTEMPT: u8 = 34;
        pub const DISARM:         u8 = 35;
        // 36 = REARM (not currently decoded)
        pub const POST:           u8 = 37;
    }
}

/// Scheduler related record identifiers (50–53).
pub mod sched {
    pub const LOCK:   u8 = 50;
    pub const UNLOCK: u8 = 51;
    pub const NEXT:   u8 = 52;
    pub const IDLE:   u8 = 53;
}
