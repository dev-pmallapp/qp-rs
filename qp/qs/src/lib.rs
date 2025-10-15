#![cfg_attr(not(feature = "std"), no_std)]

//! QS - Software Tracing Infrastructure
//!
//! Complete implementation of the QP/Spy software tracing system for embedded targets.
//! Based on the official Quantum Leaps QP/Spy specification.
//!
//! ## Features
//!
//! - **100 Predefined Record Types**: Full support for all QP framework events
//! - **Application-Specific Records**: 25 user-definable record types (100-124)
//! - **Dual Filtering**: Global (record-type) and Local (QS-ID) filters
//! - **HDLC Framing**: Complete protocol with byte-stuffing and checksums
//! - **Dictionary Support**: 5 dictionary types for symbolic debugging
//! - **Configurable Sizes**: Timestamps, pointers, signals, and counters
//! - **Zero-Overhead**: Compile-time disabled when Q_SPY not defined
//! - **no_std Compatible**: Works on bare-metal embedded targets
//!
//! ## Architecture
//!
//! The QS target component consists of:
//! - Ring buffer for trace data
//! - Global filter (128-bit, record-type based)
//! - Local filter (128-bit, QS-ID based)
//! - HDLC frame generator with byte-stuffing
//! - Configurable data type sizes
//!
//! ## Usage
//!
//! ```rust,no_run
//! use qp_qs::{init, begin, end, QSRecordType, obj_ptr, signal};
//!
//! // Initialize QS
//! init();
//!
//! // Trace a state machine transition
//! if begin(QSRecordType::QS_SM_TRAN, 0) {
//!     obj_ptr(0x20001000);  // Object pointer
//!     obj_ptr(0x08001234);  // Source state
//!     obj_ptr(0x08001256);  // Target state
//!     end();
//! }
//! ```

// Core modules
// Module declarations
mod types;
mod buffer;
mod dict;
mod macros;

// Re-exports
pub use types::{QSRecordType, QSConfig, FormatByte};

// Re-export filter and QS-ID constants
pub mod filters {
    pub use crate::types::filters::*;
}

pub mod qs_ids {
    pub use crate::types::qs_ids::*;
}

use critical_section::Mutex;
use core::cell::RefCell;
use buffer::QSBuffer;

// Global QS buffer instance
static QS_BUF: Mutex<RefCell<QSBuffer<4096>>> = Mutex::new(RefCell::new(QSBuffer::new()));

// ============================================================================
// Public API Functions
// ============================================================================

/// Initialize the QS tracing system
pub fn init() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).init();
    });
}

/// Set QS configuration
pub fn set_config(config: QSConfig) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_config(config);
    });
}

/// Get current configuration
pub fn get_config() -> QSConfig {
    critical_section::with(|cs| {
        *QS_BUF.borrow_ref(cs).config()
    })
}

/// Set global filter for a record type
pub fn global_filter(record_type: QSRecordType, enable: bool) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_global_filter(record_type, enable);
    });
}

/// Set global filter mask directly
pub fn global_filter_mask(mask: u128) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_global_filter_mask(mask);
    });
}

/// Set local filter for a QS-ID
pub fn local_filter(qs_id: u8, enable: bool) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_local_filter(qs_id, enable);
    });
}

/// Set local filter mask directly
pub fn local_filter_mask(mask: u128) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).set_local_filter_mask(mask);
    });
}

/// Begin a trace record
/// Returns true if the record passes filters and should be populated
#[cfg(not(feature = "std"))]
pub fn begin(record_type: QSRecordType, qs_id: u8) -> bool {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).begin(record_type, qs_id)
    })
}

/// End the current trace record and commit to buffer
#[cfg(not(feature = "std"))]
pub fn end() {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).end();
    });
}

// Data output functions

/// Output u8
#[cfg(not(feature = "std"))]
pub fn u8(value: u8) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u8(value);
    });
}

/// Output i8
#[cfg(not(feature = "std"))]
pub fn i8(value: i8) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).i8(value);
    });
}

/// Output u16
#[cfg(not(feature = "std"))]
pub fn u16(value: u16) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u16(value);
    });
}

/// Output i16
#[cfg(not(feature = "std"))]
pub fn i16(value: i16) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).i16(value);
    });
}

/// Output u32
#[cfg(not(feature = "std"))]
pub fn u32(value: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u32(value);
    });
}

/// Output i32
#[cfg(not(feature = "std"))]
pub fn i32(value: i32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).i32(value);
    });
}

/// Output u64
#[cfg(not(feature = "std"))]
pub fn u64(value: u64) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).u64(value);
    });
}

/// Output i64
#[cfg(not(feature = "std"))]
pub fn i64(value: i64) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).i64(value);
    });
}

/// Output f32
#[cfg(not(feature = "std"))]
pub fn f32(value: f32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).f32(value);
    });
}

/// Output f64
#[cfg(not(feature = "std"))]
pub fn f64(value: f64) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).f64(value);
    });
}

/// Output zero-terminated string
#[cfg(not(feature = "std"))]
pub fn str(value: &str) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).str(value);
    });
}

/// Output memory block
pub fn mem(data: &[u8], len: u8) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).mem(data, len);
    });
}

/// Output object pointer (uses configured obj_ptr_size)
#[cfg(not(feature = "std"))]
pub fn obj_ptr(ptr: usize) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).obj_ptr(ptr);
    });
}

/// Output function pointer (uses configured fun_ptr_size)
#[cfg(not(feature = "std"))]
pub fn fun_ptr(ptr: usize) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).fun_ptr(ptr);
    });
}

/// Output signal (uses configured signal_size)
pub fn signal(sig: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).signal(sig);
    });
}

/// Output event pointer (uses configured event_size)
pub fn evt_ptr(ptr: usize) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).evt_ptr(ptr);
    });
}

/// Output queue counter (uses configured queue_ctr_size)
pub fn queue_ctr(ctr: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).queue_ctr(ctr);
    });
}

/// Output pool counter (uses configured pool_ctr_size)
pub fn pool_ctr(ctr: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).pool_ctr(ctr);
    });
}

/// Output pool block size (uses configured pool_blk_size)
pub fn pool_blk(size: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).pool_blk(size);
    });
}

/// Output time event counter (uses configured time_evt_ctr_size)
pub fn te_ctr(ctr: u32) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).te_ctr(ctr);
    });
}

/// Read trace data from buffer
/// Returns the number of bytes read
pub fn read(buf: &mut [u8]) -> usize {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).read(buf)
    })
}

/// Get number of bytes available to read from QS buffer
pub fn available() -> usize {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref(cs).available()
    })
}

// ============================================================================
// Target Info
// ============================================================================

/// Generate QS_TARGET_INFO record
///
/// This non-maskable record contains target configuration information:
/// - QP version
/// - Endianness
/// - All 9 configurable sizes (time, signal, event, queue_ctr, pool_ctr, pool_blk, te_ctr, obj_ptr, fun_ptr)
/// - Target name string
///
/// Should be called once during initialization to inform the host about target configuration.
#[cfg(not(feature = "std"))]
pub fn target_info(qp_version: &str, target_name: &str, endianness: u8) {
    critical_section::with(|cs| {
        QS_BUF.borrow_ref_mut(cs).target_info_record(qp_version, target_name, endianness);
    });
}

/// Get QP version string for target_info
pub const QP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================================================
// Standard Library Support (when std feature is enabled)
// ============================================================================

#[cfg(feature = "std")]
mod std_impl;

#[cfg(feature = "std")]
pub use std_impl::{init_udp, init_tcp, enable, flush, target_info, begin, end, u8, u16, u32, str, obj_ptr, fun_ptr};
