#![no_std]

//! QS - Software Tracing Infrastructure
//!
//! This module provides lightweight software tracing for debugging and monitoring
//! QP applications. The tracing is designed to have minimal runtime overhead when
//! enabled and zero overhead when disabled via feature flags.
//!
//! Key features:
//! - Zero-overhead when disabled at compile time
//! - Minimal overhead when enabled
//! - Real-time trace streaming
//! - Filtering by trace record type
//! - Circular trace buffer
//! - Support for various output channels (UART, SWO, etc.)

use core::cell::RefCell;
use critical_section::Mutex;
use heapless::Deque;

/// Maximum trace buffer size
pub const QS_BUF_SIZE: usize = 256;

/// Trace record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QSRecordType {
    /// State machine initialization
    QS_SM_INIT = 0,
    /// State machine dispatch
    QS_SM_DISPATCH,
    /// State entry
    QS_SM_STATE_ENTRY,
    /// State exit
    QS_SM_STATE_EXIT,
    /// State transition
    QS_SM_TRAN,
    /// Event posted
    QS_QF_POST,
    /// Event published
    QS_QF_PUBLISH,
    /// Active object initialization
    QS_QF_ACTIVE_INIT,
    /// Tick processing
    QS_QF_TICK,
    /// Time event armed
    QS_QF_TIMEEVT_ARM,
    /// Time event disarmed
    QS_QF_TIMEEVT_DISARM,
    /// Time event post
    QS_QF_TIMEEVT_POST,
    /// Memory pool get
    QS_QF_MPOOL_GET,
    /// Memory pool put
    QS_QF_MPOOL_PUT,
    /// Custom user record
    QS_USER = 100,
}

/// QS trace buffer
pub struct QSBuffer {
    /// Circular buffer for trace data
    buffer: Deque<u8, QS_BUF_SIZE>,
    /// Filter mask for record types
    filter: u128,
    /// Enabled flag
    enabled: bool,
}

impl QSBuffer {
    /// Create a new trace buffer
    const fn new() -> Self {
        Self {
            buffer: Deque::new(),
            filter: u128::MAX, // All records enabled by default
            enabled: false,
        }
    }

    /// Initialize the trace buffer
    pub fn init(&mut self) {
        self.buffer.clear();
        self.filter = u128::MAX;
        self.enabled = true;
    }

    /// Enable tracing
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable tracing
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if tracing is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set filter for record types
    pub fn set_filter(&mut self, record_type: QSRecordType, enable: bool) {
        let bit = record_type as u8;
        if enable {
            self.filter |= 1u128 << bit;
        } else {
            self.filter &= !(1u128 << bit);
        }
    }

    /// Check if a record type is filtered
    pub fn is_filtered(&self, record_type: QSRecordType) -> bool {
        let bit = record_type as u8;
        (self.filter & (1u128 << bit)) != 0
    }

    /// Begin a trace record
    pub fn begin(&mut self, record_type: QSRecordType) -> bool {
        if !self.enabled || !self.is_filtered(record_type) {
            return false;
        }
        
        // Add record type marker
        let _ = self.buffer.push_back(record_type as u8);
        true
    }

    /// Add a u8 to the current record
    pub fn u8(&mut self, value: u8) {
        let _ = self.buffer.push_back(value);
    }

    /// Add a u16 to the current record
    pub fn u16(&mut self, value: u16) {
        let _ = self.buffer.push_back((value & 0xFF) as u8);
        let _ = self.buffer.push_back((value >> 8) as u8);
    }

    /// Add a u32 to the current record
    pub fn u32(&mut self, value: u32) {
        let _ = self.buffer.push_back((value & 0xFF) as u8);
        let _ = self.buffer.push_back(((value >> 8) & 0xFF) as u8);
        let _ = self.buffer.push_back(((value >> 16) & 0xFF) as u8);
        let _ = self.buffer.push_back((value >> 24) as u8);
    }

    /// End the current record
    pub fn end(&mut self) {
        // Add record terminator
        let _ = self.buffer.push_back(0xFF);
    }

    /// Get number of bytes in buffer
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get a byte from the buffer
    pub fn get(&mut self) -> Option<u8> {
        self.buffer.pop_front()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Global QS trace buffer
static QS_BUF: Mutex<RefCell<QSBuffer>> = Mutex::new(RefCell::new(QSBuffer::new()));

/// Initialize the QS tracing system
pub fn init() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).init();
    });
}

/// Enable tracing
pub fn enable() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).enable();
    });
}

/// Disable tracing
pub fn disable() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).disable();
    });
}

/// Check if tracing is enabled
pub fn is_enabled() -> bool {
    critical_section::with(|cs| QS_BUF.borrow_ref(cs).is_enabled())
}

/// Set filter for a record type
pub fn set_filter(record_type: QSRecordType, enable: bool) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_filter(record_type, enable);
    });
}

/// Begin a trace record
pub fn begin(record_type: QSRecordType) -> bool {
    critical_section::with(|cs| QS_BUF.borrow_ref_mut(cs).begin(record_type))
}

/// Add a u8 to the current record
pub fn u8(value: u8) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u8(value);
    });
}

/// Add a u16 to the current record
pub fn u16(value: u16) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u16(value);
    });
}

/// Add a u32 to the current record
pub fn u32(value: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u32(value);
    });
}

/// End the current record
pub fn end() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).end();
    });
}

/// Get number of bytes in buffer
pub fn len() -> usize {
    critical_section::with(|cs| QS_BUF.borrow_ref(cs).len())
}

/// Check if buffer is empty
pub fn is_empty() -> bool {
    critical_section::with(|cs| QS_BUF.borrow_ref(cs).is_empty())
}

/// Get a byte from the buffer
pub fn get() -> Option<u8> {
    critical_section::with(|cs| QS_BUF.borrow_ref_mut(cs).get())
}

/// Clear the buffer
pub fn clear() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).clear();
    });
}

/// Macro for tracing state machine initialization
#[macro_export]
macro_rules! qs_sm_init {
    ($obj:expr, $state:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_INIT) {
            $crate::u32($obj as u32);
            $crate::u32($state as u32);
            $crate::end();
        }
    };
}

/// Macro for tracing state machine dispatch
#[macro_export]
macro_rules! qs_sm_dispatch {
    ($obj:expr, $signal:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_DISPATCH) {
            $crate::u32($obj as u32);
            $crate::u16($signal);
            $crate::end();
        }
    };
}

/// Macro for tracing state transitions
#[macro_export]
macro_rules! qs_sm_tran {
    ($obj:expr, $source:expr, $target:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_TRAN) {
            $crate::u32($obj as u32);
            $crate::u32($source as u32);
            $crate::u32($target as u32);
            $crate::end();
        }
    };
}

// Tests will be added in a separate test file
