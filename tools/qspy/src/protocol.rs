//! QSpy Protocol Module
//!
//! Defines the protocol for communication between QSpy host tool and targets

use std::fmt;

/// QSpy version
pub const QSPY_VERSION: u16 = 810; // Version 8.1.0

/// Default UDP port for QSpy
pub const QSPY_UDP_PORT: u16 = 7701;

/// Timeout for socket operations (seconds)
pub const QSPY_TIMEOUT_SEC: u64 = 1;

/// Target Configuration received from QS_TARGET_INFO record
#[derive(Debug, Clone)]
pub struct TargetConfig {
    /// QP version (e.g., 0x0810 = v8.1.0)
    pub qp_version: u16,
    
    /// Build timestamp (6 bytes - seconds since epoch)
    pub build_time: [u8; 6],
    
    /// Timestamp size: 1, 2, or 4 bytes
    pub qs_time_size: u8,
    
    /// Signal size: 1, 2, or 4 bytes
    pub signal_size: u8,
    
    /// Event size: 2, 4, or 8 bytes
    pub event_size: u8,
    
    /// Queue counter size: 1, 2, or 4 bytes
    pub queue_ctr_size: u8,
    
    /// Pool counter size: 1, 2, or 4 bytes
    pub pool_ctr_size: u8,
    
    /// Pool block size: 1, 2, or 4 bytes
    pub pool_blk_size: u8,
    
    /// Time event counter size: 1, 2, or 4 bytes
    pub time_evt_ctr_size: u8,
    
    /// Object pointer size: 2, 4, or 8 bytes
    pub obj_ptr_size: u8,
    
    /// Function pointer size: 2, 4, or 8 bytes
    pub fun_ptr_size: u8,
    
    /// Target name/description
    pub target_name: String,
}

impl Default for TargetConfig {
    fn default() -> Self {
        Self {
            qp_version: 0,
            build_time: [0; 6],
            qs_time_size: 4,       // Default to 4 bytes
            signal_size: 2,        // Default to 2 bytes
            event_size: 4,         // Default to 4 bytes
            queue_ctr_size: 2,     // Default to 2 bytes
            pool_ctr_size: 2,      // Default to 2 bytes
            pool_blk_size: 2,      // Default to 2 bytes
            time_evt_ctr_size: 4,  // Default to 4 bytes
            obj_ptr_size: 8,       // Default to 8 bytes (64-bit)
            fun_ptr_size: 8,       // Default to 8 bytes (64-bit)
            target_name: String::from("Unknown"),
        }
    }
}

impl TargetConfig {
    /// Parse target configuration from QS_TARGET_INFO record data
    /// QP/C 8.1.1 format:
    /// 1. Info byte (1) - bit 7: endianness
    /// 2. QP_RELEASE (4) - u32 version 0x00MMNNPP
    /// 3. Packed sizes (5) - nibble-packed configuration
    /// 4. Bounds (2) - MAX_ACTIVE, MAX_EPOOL|MAX_TICK_RATE
    /// 5. Build time (3) - sec, min, hour
    /// 6. Build date (3) - day, month, year%100
    /// 7. Target name (optional, null-terminated)
    pub fn from_data(data: &[u8]) -> Option<Self> {
        if data.len() < 18 {  // Minimum size without target name
            return None;
        }
        
        let mut config = Self::default();
        let mut offset = 0;
        
        // 1. Info byte (endianness in bit 7)
        let _info = data[offset];
        offset += 1;
        
        // 2. QP version (4 bytes, u32 little-endian) - format: 0x00MMNNPP
        let qp_release = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]);
        // Convert to u16 for display: major*100 + minor*10 + patch
        let major = ((qp_release >> 16) & 0xFF) as u16;
        let minor = ((qp_release >> 8) & 0xFF) as u16;
        let patch = (qp_release & 0xFF) as u16;
        config.qp_version = (major * 100 + minor * 10 + patch) as u16;
        offset += 4;
        
        // 3. Packed sizes (5 bytes) - sizes are in nibbles
        // Byte 1: signal_size | event_size
        config.signal_size = data[offset] & 0x0F;
        config.event_size = (data[offset] >> 4) & 0x0F;
        offset += 1;
        
        // Byte 2: queue_ctr_size | time_evt_ctr_size
        config.queue_ctr_size = data[offset] & 0x0F;
        config.time_evt_ctr_size = (data[offset] >> 4) & 0x0F;
        offset += 1;
        
        // Byte 3: pool_blk_size | pool_ctr_size
        config.pool_blk_size = data[offset] & 0x0F;
        config.pool_ctr_size = (data[offset] >> 4) & 0x0F;
        offset += 1;
        
        // Byte 4: obj_ptr_size | fun_ptr_size
        config.obj_ptr_size = data[offset] & 0x0F;
        config.fun_ptr_size = (data[offset] >> 4) & 0x0F;
        offset += 1;
        
        // Byte 5: time_size
        config.qs_time_size = data[offset];
        offset += 1;
        
        // 4. Bounds (2 bytes) - skip for now
        offset += 2;
        
        // 5. Build time (3 bytes): sec, min, hour
        if offset + 3 <= data.len() {
            config.build_time[5] = data[offset];     // sec
            config.build_time[4] = data[offset + 1]; // min
            config.build_time[3] = data[offset + 2]; // hour
            offset += 3;
        }
        
        // 6. Build date (3 bytes): day, month, year%100
        if offset + 3 <= data.len() {
            config.build_time[2] = data[offset];     // day
            config.build_time[1] = data[offset + 1]; // month
            config.build_time[0] = data[offset + 2]; // year%100
            offset += 3;
        }
        
        // 7. Target name (remaining bytes, null-terminated)
        if offset < data.len() {
            let name_bytes = &data[offset..];
            // Find null terminator or end of data
            let name_end = name_bytes.iter().position(|&b| b == 0)
                .unwrap_or(name_bytes.len());
            config.target_name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();
        }
        
        Some(config)
    }
    
    /// Read a value based on configured size
    pub fn read_sized_value(&self, data: &[u8], size: u8) -> Option<u64> {
        match size {
            1 if data.len() >= 1 => Some(data[0] as u64),
            2 if data.len() >= 2 => Some(u16::from_le_bytes([data[0], data[1]]) as u64),
            4 if data.len() >= 4 => Some(u32::from_le_bytes([
                data[0], data[1], data[2], data[3]
            ]) as u64),
            8 if data.len() >= 8 => Some(u64::from_le_bytes([
                data[0], data[1], data[2], data[3],
                data[4], data[5], data[6], data[7]
            ])),
            _ => None,
        }
    }
    
    /// Read timestamp based on configured size
    pub fn read_timestamp(&self, data: &[u8]) -> Option<u64> {
        self.read_sized_value(data, self.qs_time_size)
    }
    
    /// Read object pointer based on configured size
    pub fn read_obj_ptr(&self, data: &[u8]) -> Option<u64> {
        self.read_sized_value(data, self.obj_ptr_size)
    }
    
    /// Read function pointer based on configured size
    pub fn read_fun_ptr(&self, data: &[u8]) -> Option<u64> {
        self.read_sized_value(data, self.fun_ptr_size)
    }
    
    /// Read signal based on configured size
    pub fn read_signal(&self, data: &[u8]) -> Option<u64> {
        self.read_sized_value(data, self.signal_size)
    }
}

/// QS Record Types - matches the target-side QS implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum QSRecordType {
    // Empty record
    QS_EMPTY = 0,

    // [1] QEP (Event Processor) records
    QS_QEP_STATE_ENTRY = 1,
    QS_QEP_STATE_EXIT = 2,
    QS_QEP_STATE_INIT = 3,
    QS_QEP_INIT_TRAN = 4,
    QS_QEP_INTERN_TRAN = 5,
    QS_QEP_TRAN = 6,
    QS_QEP_IGNORED = 7,
    QS_QEP_DISPATCH = 8,
    QS_QEP_UNHANDLED = 9,

    // [10] QF (Active Object) records
    QS_QF_ACTIVE_DEFER = 10,
    QS_QF_ACTIVE_RECALL = 11,
    QS_QF_ACTIVE_SUBSCRIBE = 12,
    QS_QF_ACTIVE_UNSUBSCRIBE = 13,
    QS_QF_ACTIVE_POST = 14,
    QS_QF_ACTIVE_POST_LIFO = 15,
    QS_QF_ACTIVE_GET = 16,
    QS_QF_ACTIVE_GET_LAST = 17,
    QS_QF_ACTIVE_RECALL_ATTEMPT = 18,

    // [19] QF (Event Queue) records
    QS_QF_EQUEUE_POST = 19,
    QS_QF_EQUEUE_POST_LIFO = 20,
    QS_QF_EQUEUE_GET = 21,
    QS_QF_EQUEUE_GET_LAST = 22,

    // [23] QF (Framework) records
    QS_QF_NEW_ATTEMPT = 23,

    // [24] Memory Pool records
    QS_QF_MPOOL_GET = 24,
    QS_QF_MPOOL_PUT = 25,

    // [26] Additional QF records
    QS_QF_PUBLISH = 26,
    QS_QF_NEW_REF = 27,
    QS_QF_NEW = 28,
    QS_QF_GC_ATTEMPT = 29,
    QS_QF_GC = 30,
    QS_QF_TICK = 31,

    // [32] Time Event records
    QS_QF_TIMEEVT_ARM = 32,
    QS_QF_TIMEEVT_AUTO_DISARM = 33,
    QS_QF_TIMEEVT_DISARM_ATTEMPT = 34,
    QS_QF_TIMEEVT_DISARM = 35,
    QS_QF_TIMEEVT_REARM = 36,
    QS_QF_TIMEEVT_POST = 37,

    // [38] Additional QF records
    QS_QF_DELETE_REF = 38,
    QS_QF_CRIT_ENTRY = 39,
    QS_QF_CRIT_EXIT = 40,
    QS_QF_ISR_ENTRY = 41,
    QS_QF_ISR_EXIT = 42,
    QS_QF_INT_DISABLE = 43,
    QS_QF_INT_ENABLE = 44,

    // [45] Additional AO records
    QS_QF_ACTIVE_POST_ATTEMPT = 45,

    // [46] Additional EQ records
    QS_QF_EQUEUE_POST_ATTEMPT = 46,

    // [47] Additional MP records
    QS_QF_MPOOL_GET_ATTEMPT = 47,

    // [48] Scheduler records
    QS_SCHED_PREEMPT = 48,
    QS_SCHED_RESTORE = 49,
    QS_SCHED_LOCK = 50,
    QS_SCHED_UNLOCK = 51,
    QS_SCHED_NEXT = 52,
    QS_SCHED_IDLE = 53,
    QS_ENUM_DICT = 54,

    // [55] Additional QEP records
    QS_QEP_TRAN_HIST = 55,
    QS_RESERVED_56 = 56,
    QS_RESERVED_57 = 57,

    // [58] Miscellaneous records
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

    // [71] Semaphore records
    QS_SEM_TAKE = 71,
    QS_SEM_BLOCK = 72,
    QS_SEM_SIGNAL = 73,
    QS_SEM_BLOCK_ATTEMPT = 74,

    // [75] Mutex records
    QS_MTX_LOCK = 75,
    QS_MTX_BLOCK = 76,
    QS_MTX_UNLOCK = 77,
    QS_MTX_LOCK_ATTEMPT = 78,
    QS_MTX_BLOCK_ATTEMPT = 79,
    QS_MTX_UNLOCK_ATTEMPT = 80,

    // [81] Additional AO records
    QS_QF_ACTIVE_DEFER_ATTEMPT = 81,

    // User records start at 100
    QS_USER = 100,
}

impl QSRecordType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::QS_EMPTY),
            1 => Some(Self::QS_QEP_STATE_ENTRY),
            2 => Some(Self::QS_QEP_STATE_EXIT),
            3 => Some(Self::QS_QEP_STATE_INIT),
            4 => Some(Self::QS_QEP_INIT_TRAN),
            5 => Some(Self::QS_QEP_INTERN_TRAN),
            6 => Some(Self::QS_QEP_TRAN),
            7 => Some(Self::QS_QEP_IGNORED),
            8 => Some(Self::QS_QEP_DISPATCH),
            9 => Some(Self::QS_QEP_UNHANDLED),
            10 => Some(Self::QS_QF_ACTIVE_DEFER),
            11 => Some(Self::QS_QF_ACTIVE_RECALL),
            12 => Some(Self::QS_QF_ACTIVE_SUBSCRIBE),
            13 => Some(Self::QS_QF_ACTIVE_UNSUBSCRIBE),
            14 => Some(Self::QS_QF_ACTIVE_POST),
            15 => Some(Self::QS_QF_ACTIVE_POST_LIFO),
            16 => Some(Self::QS_QF_ACTIVE_GET),
            17 => Some(Self::QS_QF_ACTIVE_GET_LAST),
            18 => Some(Self::QS_QF_ACTIVE_RECALL_ATTEMPT),
            19 => Some(Self::QS_QF_EQUEUE_POST),
            20 => Some(Self::QS_QF_EQUEUE_POST_LIFO),
            21 => Some(Self::QS_QF_EQUEUE_GET),
            22 => Some(Self::QS_QF_EQUEUE_GET_LAST),
            23 => Some(Self::QS_QF_NEW_ATTEMPT),
            24 => Some(Self::QS_QF_MPOOL_GET),
            25 => Some(Self::QS_QF_MPOOL_PUT),
            26 => Some(Self::QS_QF_PUBLISH),
            27 => Some(Self::QS_QF_NEW_REF),
            28 => Some(Self::QS_QF_NEW),
            29 => Some(Self::QS_QF_GC_ATTEMPT),
            30 => Some(Self::QS_QF_GC),
            31 => Some(Self::QS_QF_TICK),
            32 => Some(Self::QS_QF_TIMEEVT_ARM),
            33 => Some(Self::QS_QF_TIMEEVT_AUTO_DISARM),
            34 => Some(Self::QS_QF_TIMEEVT_DISARM_ATTEMPT),
            35 => Some(Self::QS_QF_TIMEEVT_DISARM),
            36 => Some(Self::QS_QF_TIMEEVT_REARM),
            37 => Some(Self::QS_QF_TIMEEVT_POST),
            38 => Some(Self::QS_QF_DELETE_REF),
            39 => Some(Self::QS_QF_CRIT_ENTRY),
            40 => Some(Self::QS_QF_CRIT_EXIT),
            41 => Some(Self::QS_QF_ISR_ENTRY),
            42 => Some(Self::QS_QF_ISR_EXIT),
            43 => Some(Self::QS_QF_INT_DISABLE),
            44 => Some(Self::QS_QF_INT_ENABLE),
            45 => Some(Self::QS_QF_ACTIVE_POST_ATTEMPT),
            46 => Some(Self::QS_QF_EQUEUE_POST_ATTEMPT),
            47 => Some(Self::QS_QF_MPOOL_GET_ATTEMPT),
            48 => Some(Self::QS_SCHED_PREEMPT),
            49 => Some(Self::QS_SCHED_RESTORE),
            50 => Some(Self::QS_SCHED_LOCK),
            51 => Some(Self::QS_SCHED_UNLOCK),
            52 => Some(Self::QS_SCHED_NEXT),
            53 => Some(Self::QS_SCHED_IDLE),
            54 => Some(Self::QS_ENUM_DICT),
            55 => Some(Self::QS_QEP_TRAN_HIST),
            56 => Some(Self::QS_RESERVED_56),
            57 => Some(Self::QS_RESERVED_57),
            58 => Some(Self::QS_TEST_PAUSED),
            59 => Some(Self::QS_TEST_PROBE_GET),
            60 => Some(Self::QS_SIG_DICT),
            61 => Some(Self::QS_OBJ_DICT),
            62 => Some(Self::QS_FUN_DICT),
            63 => Some(Self::QS_USR_DICT),
            64 => Some(Self::QS_TARGET_INFO),
            65 => Some(Self::QS_TARGET_DONE),
            66 => Some(Self::QS_RX_STATUS),
            67 => Some(Self::QS_QUERY_DATA),
            68 => Some(Self::QS_PEEK_DATA),
            69 => Some(Self::QS_ASSERT_FAIL),
            70 => Some(Self::QS_QF_RUN),
            71 => Some(Self::QS_SEM_TAKE),
            72 => Some(Self::QS_SEM_BLOCK),
            73 => Some(Self::QS_SEM_SIGNAL),
            74 => Some(Self::QS_SEM_BLOCK_ATTEMPT),
            75 => Some(Self::QS_MTX_LOCK),
            76 => Some(Self::QS_MTX_BLOCK),
            77 => Some(Self::QS_MTX_UNLOCK),
            78 => Some(Self::QS_MTX_LOCK_ATTEMPT),
            79 => Some(Self::QS_MTX_BLOCK_ATTEMPT),
            80 => Some(Self::QS_MTX_UNLOCK_ATTEMPT),
            81 => Some(Self::QS_QF_ACTIVE_DEFER_ATTEMPT),
            100..=255 => Some(Self::QS_USER),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::QS_EMPTY => "EMPTY",
            Self::QS_QEP_STATE_ENTRY => "SM_ENTRY",
            Self::QS_QEP_STATE_EXIT => "SM_EXIT",
            Self::QS_QEP_STATE_INIT => "SM_INIT",
            Self::QS_QEP_INIT_TRAN => "SM_INIT_TRAN",
            Self::QS_QEP_INTERN_TRAN => "SM_INTERN_TRAN",
            Self::QS_QEP_TRAN => "SM_TRAN",
            Self::QS_QEP_IGNORED => "SM_IGNORED",
            Self::QS_QEP_DISPATCH => "SM_DISPATCH",
            Self::QS_QEP_UNHANDLED => "SM_UNHANDLED",
            Self::QS_QF_ACTIVE_DEFER => "AO_DEFER",
            Self::QS_QF_ACTIVE_RECALL => "AO_RECALL",
            Self::QS_QF_ACTIVE_SUBSCRIBE => "AO_SUBSCRIBE",
            Self::QS_QF_ACTIVE_UNSUBSCRIBE => "AO_UNSUBSCRIBE",
            Self::QS_QF_ACTIVE_POST => "AO_POST",
            Self::QS_QF_ACTIVE_POST_LIFO => "AO_POST_LIFO",
            Self::QS_QF_ACTIVE_GET => "AO_GET",
            Self::QS_QF_ACTIVE_GET_LAST => "AO_GET_LAST",
            Self::QS_QF_ACTIVE_RECALL_ATTEMPT => "AO_RECALL_ATTEMPT",
            Self::QS_QF_EQUEUE_POST => "EQ_POST",
            Self::QS_QF_EQUEUE_POST_LIFO => "EQ_POST_LIFO",
            Self::QS_QF_EQUEUE_GET => "EQ_GET",
            Self::QS_QF_EQUEUE_GET_LAST => "EQ_GET_LAST",
            Self::QS_QF_NEW_ATTEMPT => "QF_NEW_ATTEMPT",
            Self::QS_QF_MPOOL_GET => "MP_GET",
            Self::QS_QF_MPOOL_PUT => "MP_PUT",
            Self::QS_QF_PUBLISH => "QF_PUBLISH",
            Self::QS_QF_NEW_REF => "QF_NEW_REF",
            Self::QS_QF_NEW => "QF_NEW",
            Self::QS_QF_GC_ATTEMPT => "QF_GC_ATTEMPT",
            Self::QS_QF_GC => "QF_GC",
            Self::QS_QF_TICK => "QF_TICK",
            Self::QS_QF_TIMEEVT_ARM => "TE_ARM",
            Self::QS_QF_TIMEEVT_AUTO_DISARM => "TE_AUTO_DISARM",
            Self::QS_QF_TIMEEVT_DISARM_ATTEMPT => "TE_DISARM_ATTEMPT",
            Self::QS_QF_TIMEEVT_DISARM => "TE_DISARM",
            Self::QS_QF_TIMEEVT_REARM => "TE_REARM",
            Self::QS_QF_TIMEEVT_POST => "TE_POST",
            Self::QS_QF_DELETE_REF => "QF_DELETE_REF",
            Self::QS_QF_CRIT_ENTRY => "QF_CRIT_ENTRY",
            Self::QS_QF_CRIT_EXIT => "QF_CRIT_EXIT",
            Self::QS_QF_ISR_ENTRY => "QF_ISR_ENTRY",
            Self::QS_QF_ISR_EXIT => "QF_ISR_EXIT",
            Self::QS_QF_INT_DISABLE => "QF_INT_DISABLE",
            Self::QS_QF_INT_ENABLE => "QF_INT_ENABLE",
            Self::QS_QF_ACTIVE_POST_ATTEMPT => "AO_POST_ATTEMPT",
            Self::QS_QF_EQUEUE_POST_ATTEMPT => "EQ_POST_ATTEMPT",
            Self::QS_QF_MPOOL_GET_ATTEMPT => "MP_GET_ATTEMPT",
            Self::QS_SCHED_PREEMPT => "SCHED_PREEMPT",
            Self::QS_SCHED_RESTORE => "SCHED_RESTORE",
            Self::QS_SCHED_LOCK => "SCHED_LOCK",
            Self::QS_SCHED_UNLOCK => "SCHED_UNLOCK",
            Self::QS_SCHED_NEXT => "SCHED_NEXT",
            Self::QS_SCHED_IDLE => "SCHED_IDLE",
            Self::QS_ENUM_DICT => "ENUM_DICT",
            Self::QS_QEP_TRAN_HIST => "SM_TRAN_HIST",
            Self::QS_RESERVED_56 => "RESERVED_56",
            Self::QS_RESERVED_57 => "RESERVED_57",
            Self::QS_TEST_PAUSED => "TEST_PAUSED",
            Self::QS_TEST_PROBE_GET => "TEST_PROBE_GET",
            Self::QS_SIG_DICT => "SIG_DICT",
            Self::QS_OBJ_DICT => "OBJ_DICT",
            Self::QS_FUN_DICT => "FUN_DICT",
            Self::QS_USR_DICT => "USR_DICT",
            Self::QS_TARGET_INFO => "TARGET_INFO",
            Self::QS_TARGET_DONE => "TARGET_DONE",
            Self::QS_RX_STATUS => "RX_STATUS",
            Self::QS_QUERY_DATA => "QUERY_DATA",
            Self::QS_PEEK_DATA => "PEEK_DATA",
            Self::QS_ASSERT_FAIL => "ASSERT_FAIL",
            Self::QS_QF_RUN => "QF_RUN",
            Self::QS_SEM_TAKE => "SEM_TAKE",
            Self::QS_SEM_BLOCK => "SEM_BLOCK",
            Self::QS_SEM_SIGNAL => "SEM_SIGNAL",
            Self::QS_SEM_BLOCK_ATTEMPT => "SEM_BLOCK_ATTEMPT",
            Self::QS_MTX_LOCK => "MTX_LOCK",
            Self::QS_MTX_BLOCK => "MTX_BLOCK",
            Self::QS_MTX_UNLOCK => "MTX_UNLOCK",
            Self::QS_MTX_LOCK_ATTEMPT => "MTX_LOCK_ATTEMPT",
            Self::QS_MTX_BLOCK_ATTEMPT => "MTX_BLOCK_ATTEMPT",
            Self::QS_MTX_UNLOCK_ATTEMPT => "MTX_UNLOCK_ATTEMPT",
            Self::QS_QF_ACTIVE_DEFER_ATTEMPT => "AO_DEFER_ATTEMPT",
            Self::QS_USER => "USER",
        }
    }

    pub fn group(&self) -> RecordGroup {
        match self {
            Self::QS_EMPTY | Self::QS_TARGET_INFO | Self::QS_QF_RUN => RecordGroup::Info,
            Self::QS_QEP_STATE_ENTRY | Self::QS_QEP_STATE_EXIT | Self::QS_QEP_STATE_INIT 
            | Self::QS_QEP_INIT_TRAN | Self::QS_QEP_INTERN_TRAN | Self::QS_QEP_TRAN 
            | Self::QS_QEP_IGNORED | Self::QS_QEP_DISPATCH | Self::QS_QEP_UNHANDLED
            | Self::QS_QEP_TRAN_HIST => RecordGroup::StateMachine,
            Self::QS_QF_ACTIVE_DEFER | Self::QS_QF_ACTIVE_RECALL | Self::QS_QF_ACTIVE_SUBSCRIBE 
            | Self::QS_QF_ACTIVE_UNSUBSCRIBE | Self::QS_QF_ACTIVE_POST | Self::QS_QF_ACTIVE_POST_LIFO
            | Self::QS_QF_ACTIVE_GET | Self::QS_QF_ACTIVE_GET_LAST | Self::QS_QF_ACTIVE_RECALL_ATTEMPT 
            | Self::QS_QF_ACTIVE_POST_ATTEMPT | Self::QS_QF_ACTIVE_DEFER_ATTEMPT => RecordGroup::ActiveObject,
            Self::QS_QF_EQUEUE_POST | Self::QS_QF_EQUEUE_POST_LIFO 
            | Self::QS_QF_EQUEUE_GET | Self::QS_QF_EQUEUE_GET_LAST 
            | Self::QS_QF_EQUEUE_POST_ATTEMPT => RecordGroup::EventQueue,
            Self::QS_QF_MPOOL_GET | Self::QS_QF_MPOOL_PUT | Self::QS_QF_MPOOL_GET_ATTEMPT => RecordGroup::MemoryPool,
            Self::QS_QF_TIMEEVT_ARM | Self::QS_QF_TIMEEVT_AUTO_DISARM 
            | Self::QS_QF_TIMEEVT_DISARM_ATTEMPT | Self::QS_QF_TIMEEVT_DISARM
            | Self::QS_QF_TIMEEVT_REARM | Self::QS_QF_TIMEEVT_POST => RecordGroup::TimeEvent,
            Self::QS_SCHED_PREEMPT | Self::QS_SCHED_RESTORE | Self::QS_SCHED_LOCK
            | Self::QS_SCHED_UNLOCK | Self::QS_SCHED_NEXT | Self::QS_SCHED_IDLE => RecordGroup::Scheduler,
            Self::QS_SEM_TAKE | Self::QS_SEM_BLOCK | Self::QS_SEM_SIGNAL
            | Self::QS_SEM_BLOCK_ATTEMPT => RecordGroup::Semaphore,
            Self::QS_MTX_LOCK | Self::QS_MTX_BLOCK | Self::QS_MTX_UNLOCK
            | Self::QS_MTX_LOCK_ATTEMPT | Self::QS_MTX_BLOCK_ATTEMPT
            | Self::QS_MTX_UNLOCK_ATTEMPT => RecordGroup::Mutex,
            Self::QS_SIG_DICT | Self::QS_OBJ_DICT | Self::QS_FUN_DICT 
            | Self::QS_USR_DICT | Self::QS_ENUM_DICT => RecordGroup::Dictionary,
            Self::QS_TEST_PAUSED | Self::QS_TEST_PROBE_GET | Self::QS_TARGET_DONE 
            | Self::QS_RX_STATUS | Self::QS_QUERY_DATA | Self::QS_PEEK_DATA => RecordGroup::Test,
            Self::QS_ASSERT_FAIL | Self::QS_RESERVED_56 | Self::QS_RESERVED_57 => RecordGroup::Error,
            Self::QS_USER => RecordGroup::User,
            _ => RecordGroup::Framework,
        }
    }
}

impl fmt::Display for QSRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Record group for coloring/filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordGroup {
    Info,
    StateMachine,
    ActiveObject,
    EventQueue,
    MemoryPool,
    TimeEvent,
    Scheduler,
    Semaphore,
    Mutex,
    Framework,
    Dictionary,
    Test,
    Error,
    User,
}

/// QSpy command packets
#[repr(u8)]
#[allow(dead_code)]
pub enum QSpyCommand {
    Attach = 128,
    Detach = 129,
    SaveDict = 130,
    TextOut = 131,
    BinOut = 132,
    MatlabOut = 133,
    SequenceOut = 134,
    ClearScreen = 140,
    ShowNote = 141,
}

/// Commands to target
#[repr(u8)]
#[allow(dead_code)]
pub enum TargetCommand {
    Info = 0,
    Reset = 2,
}

/// QS trace record from target
#[derive(Debug, Clone)]
pub struct QSRecord {
    pub timestamp: u64,
    pub record_type: QSRecordType,
    pub data: Vec<u8>,
}

impl QSRecord {
    pub fn new(timestamp: u64, record_type: QSRecordType, data: Vec<u8>) -> Self {
        Self {
            timestamp,
            record_type,
            data,
        }
    }
}
