//! Canonical QS record identifiers shared across the workspace.

/// QEP (state machine) related record identifiers.
pub mod qep {
    /// State entry record (`QS_QEP_STATE_ENTRY`).
    pub const STATE_ENTRY: u8 = 1;
    /// State exit record (`QS_QEP_STATE_EXIT`).
    pub const STATE_EXIT: u8 = 2;
    /// Initial transition record (`QS_QEP_STATE_INIT`).
    pub const STATE_INIT: u8 = 3;
    /// Top-level initial transition record (`QS_QEP_INIT_TRAN`).
    pub const INIT_TRAN: u8 = 4;
    /// Internal transition record (`QS_QEP_INTERN_TRAN`).
    pub const INTERN_TRAN: u8 = 5;
    /// External transition record (`QS_QEP_TRAN`).
    pub const TRAN: u8 = 6;
    /// Signal ignored record (`QS_QEP_IGNORED`).
    pub const IGNORED: u8 = 7;
    /// Dispatch record (`QS_QEP_DISPATCH`).
    pub const DISPATCH: u8 = 8;
    /// Unhandled signal record (`QS_QEP_UNHANDLED`).
    pub const UNHANDLED: u8 = 9;
}

/// QF (framework) record identifiers.
pub mod qf {
    pub mod time_evt {
        /// Time event armed record (`QS_QF_TIMEEVT_ARM`).
        pub const ARM: u8 = 32;
        /// Time event automatically disarmed (`QS_QF_TIMEEVT_AUTO_DISARM`).
        pub const AUTO_DISARM: u8 = 33;
        /// Failed attempt to disarm (`QS_QF_TIMEEVT_DISARM_ATTEMPT`).
        pub const DISARM_ATTEMPT: u8 = 34;
        /// Time event disarmed (`QS_QF_TIMEEVT_DISARM`).
        pub const DISARM: u8 = 35;
        /// Time event posted (`QS_QF_TIMEEVT_POST`).
        pub const POST: u8 = 37;
    }
}

/// Scheduler related record identifiers.
pub mod sched {
    /// Scheduler lock record (`QS_SCHED_LOCK`).
    pub const LOCK: u8 = 50;
    /// Scheduler unlock record (`QS_SCHED_UNLOCK`).
    pub const UNLOCK: u8 = 51;
    /// Scheduler next record (`QS_SCHED_NEXT`).
    pub const NEXT: u8 = 52;
    /// Scheduler idle record (`QS_SCHED_IDLE`).
    pub const IDLE: u8 = 53;
}
