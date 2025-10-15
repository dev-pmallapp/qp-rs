//! QS Type Definitions
//!
//! Core types for the QS software tracing system matching the official
//! QP/Spy specification from Quantum Leaps QP/C 8.1.1.

/// All predefined QS record types
/// Based on QP/C 8.1.1 enum QS_GlbPre in qs.h
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum QSRecordType {
    // [0] QS session (not maskable)
    QS_EMPTY = 0,

    // [1-9] SM records
    QS_QEP_STATE_ENTRY = 1,
    QS_QEP_STATE_EXIT = 2,
    QS_QEP_STATE_INIT = 3,
    QS_QEP_INIT_TRAN = 4,
    QS_QEP_INTERN_TRAN = 5,
    QS_QEP_TRAN = 6,
    QS_QEP_IGNORED = 7,
    QS_QEP_DISPATCH = 8,
    QS_QEP_UNHANDLED = 9,

    // [10-18] Active Object (AO) records
    QS_QF_ACTIVE_DEFER = 10,
    QS_QF_ACTIVE_RECALL = 11,
    QS_QF_ACTIVE_SUBSCRIBE = 12,
    QS_QF_ACTIVE_UNSUBSCRIBE = 13,
    QS_QF_ACTIVE_POST = 14,
    QS_QF_ACTIVE_POST_LIFO = 15,
    QS_QF_ACTIVE_GET = 16,
    QS_QF_ACTIVE_GET_LAST = 17,
    QS_QF_ACTIVE_RECALL_ATTEMPT = 18,

    // [19-22] Event Queue (EQ) records
    QS_QF_EQUEUE_POST = 19,
    QS_QF_EQUEUE_POST_LIFO = 20,
    QS_QF_EQUEUE_GET = 21,
    QS_QF_EQUEUE_GET_LAST = 22,

    // [23] Framework (QF) records
    QS_QF_NEW_ATTEMPT = 23,

    // [24-25] Memory Pool (MP) records
    QS_QF_MPOOL_GET = 24,
    QS_QF_MPOOL_PUT = 25,

    // [26-31] Additional Framework (QF) records
    QS_QF_PUBLISH = 26,
    QS_QF_NEW_REF = 27,
    QS_QF_NEW = 28,
    QS_QF_GC_ATTEMPT = 29,
    QS_QF_GC = 30,
    QS_QF_TICK = 31,

    // [32-37] Time Event (TE) records
    QS_QF_TIMEEVT_ARM = 32,
    QS_QF_TIMEEVT_AUTO_DISARM = 33,
    QS_QF_TIMEEVT_DISARM_ATTEMPT = 34,
    QS_QF_TIMEEVT_DISARM = 35,
    QS_QF_TIMEEVT_REARM = 36,
    QS_QF_TIMEEVT_POST = 37,

    // [38-47] Additional Framework (QF/AO/EQ/MP) records
    QS_QF_DELETE_REF = 38,
    QS_QF_CRIT_ENTRY = 39,
    QS_QF_CRIT_EXIT = 40,
    QS_QF_ISR_ENTRY = 41,
    QS_QF_ISR_EXIT = 42,
    QS_QF_INT_DISABLE = 43,
    QS_QF_INT_ENABLE = 44,
    QS_QF_ACTIVE_POST_ATTEMPT = 45,
    QS_QF_EQUEUE_POST_ATTEMPT = 46,
    QS_QF_MPOOL_GET_ATTEMPT = 47,

    // [48-53] Scheduler (SC) records
    QS_SCHED_PREEMPT = 48,
    QS_SCHED_RESTORE = 49,
    QS_SCHED_LOCK = 50,
    QS_SCHED_UNLOCK = 51,
    QS_SCHED_NEXT = 52,
    QS_SCHED_IDLE = 53,

    // [54] Miscellaneous QS records (not maskable)
    QS_ENUM_DICT = 54,

    // [55-57] Additional QEP records
    QS_QEP_TRAN_HIST = 55,
    QS_RESERVED_56 = 56,
    QS_RESERVED_57 = 57,

    // [58-70] Miscellaneous QS records (not maskable)
    QS_TEST_PAUSED = 58,
    QS_TEST_PROBE_GET = 59,
    QS_SIG_DICT = 60,
    QS_OBJ_DICT = 61,
    QS_FUN_DICT = 62,
    QS_USR_DICT = 63,
    QS_TARGET_INFO = 64,
    QS_TARGET_DONE = 65,
    QS_RX_STATUS = 66,
    QS_QUERY_DATA = 67,
    QS_PEEK_DATA = 68,
    QS_ASSERT_FAIL = 69,
    QS_QF_RUN = 70,

    // [71-80] Semaphore (SEM) and Mutex (MTX) records
    QS_SEM_TAKE = 71,
    QS_SEM_BLOCK = 72,
    QS_SEM_SIGNAL = 73,
    QS_SEM_BLOCK_ATTEMPT = 74,
    QS_MTX_LOCK = 75,
    QS_MTX_BLOCK = 76,
    QS_MTX_UNLOCK = 77,
    QS_MTX_LOCK_ATTEMPT = 78,
    QS_MTX_BLOCK_ATTEMPT = 79,
    QS_MTX_UNLOCK_ATTEMPT = 80,

    // [81] Additional QF (AO) records
    QS_QF_ACTIVE_DEFER_ATTEMPT = 81,

    // User Records (100+)
    QS_USER = 100,
}

impl QSRecordType {
    /// Get the record type name
    pub const fn name(self) -> &'static str {
        match self {
            Self::QS_EMPTY => "QS_EMPTY",
            Self::QS_QEP_STATE_ENTRY => "QS_QEP_STATE_ENTRY",
            Self::QS_QEP_STATE_EXIT => "QS_QEP_STATE_EXIT",
            Self::QS_QEP_STATE_INIT => "QS_QEP_STATE_INIT",
            Self::QS_QEP_INIT_TRAN => "QS_QEP_INIT_TRAN",
            Self::QS_QEP_INTERN_TRAN => "QS_QEP_INTERN_TRAN",
            Self::QS_QEP_TRAN => "QS_QEP_TRAN",
            Self::QS_QEP_IGNORED => "QS_QEP_IGNORED",
            Self::QS_QEP_DISPATCH => "QS_QEP_DISPATCH",
            Self::QS_QEP_UNHANDLED => "QS_QEP_UNHANDLED",
            Self::QS_QF_ACTIVE_DEFER => "QS_QF_ACTIVE_DEFER",
            Self::QS_QF_ACTIVE_RECALL => "QS_QF_ACTIVE_RECALL",
            Self::QS_QF_ACTIVE_SUBSCRIBE => "QS_QF_ACTIVE_SUBSCRIBE",
            Self::QS_QF_ACTIVE_UNSUBSCRIBE => "QS_QF_ACTIVE_UNSUBSCRIBE",
            Self::QS_QF_ACTIVE_POST => "QS_QF_ACTIVE_POST",
            Self::QS_QF_ACTIVE_POST_LIFO => "QS_QF_ACTIVE_POST_LIFO",
            Self::QS_QF_ACTIVE_GET => "QS_QF_ACTIVE_GET",
            Self::QS_QF_ACTIVE_GET_LAST => "QS_QF_ACTIVE_GET_LAST",
            Self::QS_QF_ACTIVE_RECALL_ATTEMPT => "QS_QF_ACTIVE_RECALL_ATTEMPT",
            Self::QS_QF_EQUEUE_POST => "QS_QF_EQUEUE_POST",
            Self::QS_QF_EQUEUE_POST_LIFO => "QS_QF_EQUEUE_POST_LIFO",
            Self::QS_QF_EQUEUE_GET => "QS_QF_EQUEUE_GET",
            Self::QS_QF_EQUEUE_GET_LAST => "QS_QF_EQUEUE_GET_LAST",
            Self::QS_QF_NEW_ATTEMPT => "QS_QF_NEW_ATTEMPT",
            Self::QS_QF_MPOOL_GET => "QS_QF_MPOOL_GET",
            Self::QS_QF_MPOOL_PUT => "QS_QF_MPOOL_PUT",
            Self::QS_QF_PUBLISH => "QS_QF_PUBLISH",
            Self::QS_QF_NEW_REF => "QS_QF_NEW_REF",
            Self::QS_QF_NEW => "QS_QF_NEW",
            Self::QS_QF_GC_ATTEMPT => "QS_QF_GC_ATTEMPT",
            Self::QS_QF_GC => "QS_QF_GC",
            Self::QS_QF_TICK => "QS_QF_TICK",
            Self::QS_QF_TIMEEVT_ARM => "QS_QF_TIMEEVT_ARM",
            Self::QS_QF_TIMEEVT_AUTO_DISARM => "QS_QF_TIMEEVT_AUTO_DISARM",
            Self::QS_QF_TIMEEVT_DISARM_ATTEMPT => "QS_QF_TIMEEVT_DISARM_ATTEMPT",
            Self::QS_QF_TIMEEVT_DISARM => "QS_QF_TIMEEVT_DISARM",
            Self::QS_QF_TIMEEVT_REARM => "QS_QF_TIMEEVT_REARM",
            Self::QS_QF_TIMEEVT_POST => "QS_QF_TIMEEVT_POST",
            Self::QS_QF_DELETE_REF => "QS_QF_DELETE_REF",
            Self::QS_QF_CRIT_ENTRY => "QS_QF_CRIT_ENTRY",
            Self::QS_QF_CRIT_EXIT => "QS_QF_CRIT_EXIT",
            Self::QS_QF_ISR_ENTRY => "QS_QF_ISR_ENTRY",
            Self::QS_QF_ISR_EXIT => "QS_QF_ISR_EXIT",
            Self::QS_QF_INT_DISABLE => "QS_QF_INT_DISABLE",
            Self::QS_QF_INT_ENABLE => "QS_QF_INT_ENABLE",
            Self::QS_QF_ACTIVE_POST_ATTEMPT => "QS_QF_ACTIVE_POST_ATTEMPT",
            Self::QS_QF_EQUEUE_POST_ATTEMPT => "QS_QF_EQUEUE_POST_ATTEMPT",
            Self::QS_QF_MPOOL_GET_ATTEMPT => "QS_QF_MPOOL_GET_ATTEMPT",
            Self::QS_SCHED_PREEMPT => "QS_SCHED_PREEMPT",
            Self::QS_SCHED_RESTORE => "QS_SCHED_RESTORE",
            Self::QS_SCHED_LOCK => "QS_SCHED_LOCK",
            Self::QS_SCHED_UNLOCK => "QS_SCHED_UNLOCK",
            Self::QS_SCHED_NEXT => "QS_SCHED_NEXT",
            Self::QS_SCHED_IDLE => "QS_SCHED_IDLE",
            Self::QS_ENUM_DICT => "QS_ENUM_DICT",
            Self::QS_QEP_TRAN_HIST => "QS_QEP_TRAN_HIST",
            Self::QS_RESERVED_56 => "QS_RESERVED_56",
            Self::QS_RESERVED_57 => "QS_RESERVED_57",
            Self::QS_TEST_PAUSED => "QS_TEST_PAUSED",
            Self::QS_TEST_PROBE_GET => "QS_TEST_PROBE_GET",
            Self::QS_SIG_DICT => "QS_SIG_DICT",
            Self::QS_OBJ_DICT => "QS_OBJ_DICT",
            Self::QS_FUN_DICT => "QS_FUN_DICT",
            Self::QS_USR_DICT => "QS_USR_DICT",
            Self::QS_TARGET_INFO => "QS_TARGET_INFO",
            Self::QS_TARGET_DONE => "QS_TARGET_DONE",
            Self::QS_RX_STATUS => "QS_RX_STATUS",
            Self::QS_QUERY_DATA => "QS_QUERY_DATA",
            Self::QS_PEEK_DATA => "QS_PEEK_DATA",
            Self::QS_ASSERT_FAIL => "QS_ASSERT_FAIL",
            Self::QS_QF_RUN => "QS_QF_RUN",
            Self::QS_SEM_TAKE => "QS_SEM_TAKE",
            Self::QS_SEM_BLOCK => "QS_SEM_BLOCK",
            Self::QS_SEM_SIGNAL => "QS_SEM_SIGNAL",
            Self::QS_SEM_BLOCK_ATTEMPT => "QS_SEM_BLOCK_ATTEMPT",
            Self::QS_MTX_LOCK => "QS_MTX_LOCK",
            Self::QS_MTX_BLOCK => "QS_MTX_BLOCK",
            Self::QS_MTX_UNLOCK => "QS_MTX_UNLOCK",
            Self::QS_MTX_LOCK_ATTEMPT => "QS_MTX_LOCK_ATTEMPT",
            Self::QS_MTX_BLOCK_ATTEMPT => "QS_MTX_BLOCK_ATTEMPT",
            Self::QS_MTX_UNLOCK_ATTEMPT => "QS_MTX_UNLOCK_ATTEMPT",
            Self::QS_QF_ACTIVE_DEFER_ATTEMPT => "QS_QF_ACTIVE_DEFER_ATTEMPT",
            Self::QS_USER => "QS_USER",
        }
    }

    /// Check if record is non-maskable (always passes filters)
    pub const fn is_non_maskable(self) -> bool {
        matches!(
            self,
            Self::QS_SIG_DICT
                | Self::QS_OBJ_DICT
                | Self::QS_FUN_DICT
                | Self::QS_USR_DICT
                | Self::QS_ENUM_DICT
                | Self::QS_TARGET_INFO
                | Self::QS_TARGET_DONE
                | Self::QS_RX_STATUS
                | Self::QS_QUERY_DATA
                | Self::QS_PEEK_DATA
                | Self::QS_ASSERT_FAIL
                | Self::QS_QF_RUN
                | Self::QS_TEST_PAUSED
                | Self::QS_TEST_PROBE_GET
                | Self::QS_EMPTY
        )
    }
}

/// Configuration for QS target
#[derive(Debug, Clone, Copy)]
pub struct QSConfig {
    /// Timestamp size in bytes (1, 2, or 4)
    pub time_size: u8,
    /// Signal size in bytes (1, 2, or 4)
    pub signal_size: u8,
    /// Event size in bytes (2, 4, or 8)
    pub event_size: u8,
    /// Queue counter size in bytes (1, 2, or 4)
    pub queue_ctr_size: u8,
    /// Pool counter size in bytes (1, 2, or 4)
    pub pool_ctr_size: u8,
    /// Pool block size in bytes (1, 2, or 4)
    pub pool_blk_size: u8,
    /// Time event counter size in bytes (1, 2, or 4)
    pub time_evt_ctr_size: u8,
    /// Object pointer size in bytes (2, 4, or 8)
    pub obj_ptr_size: u8,
    /// Function pointer size in bytes (2, 4, or 8)
    pub fun_ptr_size: u8,
}

impl Default for QSConfig {
    fn default() -> Self {
        Self {
            time_size: 4,
            signal_size: 2,
            event_size: 4,
            queue_ctr_size: 2,
            pool_ctr_size: 2,
            pool_blk_size: 2,
            time_evt_ctr_size: 4,
            obj_ptr_size: core::mem::size_of::<usize>() as u8,
            fun_ptr_size: core::mem::size_of::<usize>() as u8,
        }
    }
}

/// Format byte for application-specific records
/// Lower nibble: data type (0-15)
/// Upper nibble: format width (0-15 digits)
#[derive(Debug, Clone, Copy)]
pub struct FormatByte(pub u8);

impl FormatByte {
    /// Data types for format byte (lower nibble)
    pub const I8: u8 = 0;
    pub const U8: u8 = 1;
    pub const I16: u8 = 2;
    pub const U16: u8 = 3;
    pub const I32: u8 = 4;
    pub const U32: u8 = 5;
    pub const F32: u8 = 6;
    pub const F64: u8 = 7;
    pub const STR: u8 = 8;
    pub const MEM: u8 = 9;
    pub const SIG: u8 = 10;
    pub const OBJ: u8 = 11;
    pub const FUN: u8 = 12;
    pub const I64: u8 = 13;
    pub const U64: u8 = 14;

    /// Create format byte from type and width
    pub const fn new(data_type: u8, width: u8) -> Self {
        Self((width << 4) | (data_type & 0x0F))
    }

    /// Get data type (lower nibble)
    pub const fn data_type(self) -> u8 {
        self.0 & 0x0F
    }

    /// Get format width (upper nibble)
    pub const fn width(self) -> u8 {
        self.0 >> 4
    }
}

/// Record group filters
pub mod filters {
    /// All records
    pub const ALL_RECORDS: u128 = u128::MAX;

    /// State Machine records group
    pub const SM_RECORDS: u128 = 0x3FF; // Bits 0-9

    /// Active Object records group
    pub const AO_RECORDS: u128 = 0xFC00; // Bits 10-15

    /// Event Queue records group
    pub const EQ_RECORDS: u128 = 0xF00000; // Bits 20-23

    /// Memory Pool records group
    pub const MP_RECORDS: u128 = 0x3000000; // Bits 24-25

    /// Time Event records group
    pub const TE_RECORDS: u128 = 0x7F00000000; // Bits 30-36

    /// QF records group
    pub const QF_RECORDS: u128 = 0x3F0000000000; // Bits 40-45

    /// Scheduler records group
    pub const SC_RECORDS: u128 = 0x1F000000000000000; // Bits 60-64

    /// User group 0 (QS_USER+0 to QS_USER+4)
    pub const U0_RECORDS: u128 = 0x1F << 100;

    /// User group 1 (QS_USER+5 to QS_USER+9)
    pub const U1_RECORDS: u128 = 0x1F << 105;

    /// User group 2 (QS_USER+10 to QS_USER+14)
    pub const U2_RECORDS: u128 = 0x1F << 110;

    /// User group 3 (QS_USER+15 to QS_USER+19)
    pub const U3_RECORDS: u128 = 0x1F << 115;

    /// User group 4 (QS_USER+20 to QS_USER+24)
    pub const U4_RECORDS: u128 = 0x1F << 120;

    /// All user records (QS_USER+0 to QS_USER+24)
    pub const UA_RECORDS: u128 = 0x1FFFFFF << 100;
}

/// QS-ID ranges for local filter
pub mod qs_ids {
    use core::ops::RangeInclusive;

    /// Active Object priorities (1-64)
    pub const AO_IDS: RangeInclusive<u8> = 1..=64;

    /// Event Pool IDs (65-80)
    pub const EP_IDS: RangeInclusive<u8> = 65..=80;

    /// Event Queue IDs (81-96)
    pub const EQ_IDS: RangeInclusive<u8> = 81..=96;

    /// Application-specific IDs (97-127)
    pub const AP_IDS: RangeInclusive<u8> = 97..=127;

    /// Convert a QS-ID range to a 128-bit filter mask
    pub const fn range_to_mask(range: &RangeInclusive<u8>) -> u128 {
        let start = *range.start() as u32;
        let end = *range.end() as u32;
        let count = end - start + 1;
        
        // Create mask with 'count' bits set, shifted to 'start' position
        if count >= 128 {
            u128::MAX
        } else {
            let mask = (1u128 << count) - 1;
            mask << start
        }
    }

    /// Get mask for AO IDs (1-64)
    pub const AO_MASK: u128 = {
        // Manually compute: bits 1-64 set
        (u128::MAX >> 64) << 1
    };

    /// Get mask for EP IDs (65-80)
    pub const EP_MASK: u128 = {
        // Bits 65-80 set
        0xFFFF << 65
    };

    /// Get mask for EQ IDs (81-96)
    pub const EQ_MASK: u128 = {
        // Bits 81-96 set
        0xFFFF << 81
    };

    /// Get mask for AP IDs (97-127)
    pub const AP_MASK: u128 = {
        // Bits 97-127 set (31 bits)
        0x7FFF_FFFF << 97
    };
}
