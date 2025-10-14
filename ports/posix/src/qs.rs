//! QS (Quantum Spy) Tracing for POSIX Port
//!
//! Platform-specific QS implementation that outputs traces to stdout/stderr
//! using standard library I/O facilities.

use std::io::{self, Write};
use std::sync::Mutex;

/// QS trace record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QSRecordType {
    // State Machine records
    QS_SM_INIT = 1,
    QS_SM_DISPATCH = 2,
    QS_SM_TRAN = 3,
    QS_SM_STATE_ENTRY = 4,
    QS_SM_STATE_EXIT = 5,
    
    // Active Object records
    QS_AO_POST = 10,
    QS_AO_GET = 11,
    
    // Time Event records
    QS_TE_ARM = 20,
    QS_TE_DISARM = 21,
    QS_TE_POST = 22,
    
    // Scheduler records
    QS_SCHED_LOCK = 30,
    QS_SCHED_UNLOCK = 31,
    QS_SCHED_IDLE = 32,
    
    // User-defined records
    QS_USER = 100,
}

/// QS output configuration
struct QSOutput {
    enabled: bool,
    filter: u64, // Bitmask for filtering record types
}

static QS_OUTPUT: Mutex<QSOutput> = Mutex::new(QSOutput {
    enabled: true,
    filter: 0xFFFFFFFFFFFFFFFF, // All enabled by default
});

/// Initialize QS tracing
pub fn init() {
    let mut output = QS_OUTPUT.lock().unwrap();
    output.enabled = true;
    println!("QS: Tracing initialized (POSIX port)");
}

/// Enable QS tracing
pub fn enable() {
    let mut output = QS_OUTPUT.lock().unwrap();
    output.enabled = true;
}

/// Disable QS tracing
pub fn disable() {
    let mut output = QS_OUTPUT.lock().unwrap();
    output.enabled = false;
}

/// Check if a record type is enabled
pub fn is_enabled(record: QSRecordType) -> bool {
    let output = QS_OUTPUT.lock().unwrap();
    if !output.enabled {
        return false;
    }
    let bit = 1u64 << (record as u8);
    (output.filter & bit) != 0
}

/// Set filter for record types
pub fn set_filter(filter: u64) {
    let mut output = QS_OUTPUT.lock().unwrap();
    output.filter = filter;
}

/// Begin a QS trace record
pub fn begin(record: QSRecordType) {
    if !is_enabled(record) {
        return;
    }
    
    print!("QS: [{:?}] ", record);
    io::stdout().flush().ok();
}

/// End a QS trace record
pub fn end() {
    println!();
}

/// Output a string
pub fn str(s: &str) {
    print!("{}", s);
    io::stdout().flush().ok();
}

/// Output an unsigned integer
pub fn u8(val: u8) {
    print!("{} ", val);
    io::stdout().flush().ok();
}

/// Output a 16-bit unsigned integer
pub fn u16(val: u16) {
    print!("{} ", val);
    io::stdout().flush().ok();
}

/// Output a 32-bit unsigned integer
pub fn u32(val: u32) {
    print!("{} ", val);
    io::stdout().flush().ok();
}

/// Output a signed integer
pub fn i32(val: i32) {
    print!("{} ", val);
    io::stdout().flush().ok();
}

/// Output a pointer address
pub fn ptr(ptr: *const ()) {
    print!("{:p} ", ptr);
    io::stdout().flush().ok();
}

/// Convenience macro for QS tracing
#[macro_export]
macro_rules! qs_trace {
    ($record:expr, $($arg:tt)*) => {{
        if $crate::qs::is_enabled($record) {
            $crate::qs::begin($record);
            print!($($arg)*);
            $crate::qs::end();
        }
    }};
}

/// State machine initialization trace
pub fn sm_init(sm_name: &str, state_name: &str) {
    if !is_enabled(QSRecordType::QS_SM_INIT) {
        return;
    }
    begin(QSRecordType::QS_SM_INIT);
    print!("SM={} INIT={}", sm_name, state_name);
    end();
}

/// State machine transition trace
pub fn sm_tran(sm_name: &str, from: &str, to: &str) {
    if !is_enabled(QSRecordType::QS_SM_TRAN) {
        return;
    }
    begin(QSRecordType::QS_SM_TRAN);
    print!("SM={} FROM={} TO={}", sm_name, from, to);
    end();
}

/// State machine dispatch trace
pub fn sm_dispatch(sm_name: &str, state: &str, signal: u16) {
    if !is_enabled(QSRecordType::QS_SM_DISPATCH) {
        return;
    }
    begin(QSRecordType::QS_SM_DISPATCH);
    print!("SM={} STATE={} SIG={}", sm_name, state, signal);
    end();
}

/// Active object post trace
pub fn ao_post(ao_name: &str, signal: u16, queue_len: usize) {
    if !is_enabled(QSRecordType::QS_AO_POST) {
        return;
    }
    begin(QSRecordType::QS_AO_POST);
    print!("AO={} SIG={} QLEN={}", ao_name, signal, queue_len);
    end();
}

/// User-defined trace
pub fn user(id: u8, msg: &str) {
    if !is_enabled(QSRecordType::QS_USER) {
        return;
    }
    begin(QSRecordType::QS_USER);
    print!("ID={} MSG={}", id, msg);
    end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qs_init() {
        init();
        assert!(is_enabled(QSRecordType::QS_SM_INIT));
    }

    #[test]
    fn test_qs_enable_disable() {
        enable();
        assert!(is_enabled(QSRecordType::QS_SM_INIT));
        
        disable();
        assert!(!is_enabled(QSRecordType::QS_SM_INIT));
        
        enable();
    }

    #[test]
    fn test_qs_filter() {
        set_filter(0); // Disable all
        assert!(!is_enabled(QSRecordType::QS_SM_INIT));
        
        set_filter(0xFFFFFFFFFFFFFFFF); // Enable all
        assert!(is_enabled(QSRecordType::QS_SM_INIT));
    }

    #[test]
    fn test_qs_traces() {
        init();
        sm_init("TestSM", "InitialState");
        sm_tran("TestSM", "StateA", "StateB");
        sm_dispatch("TestSM", "StateB", 1);
        ao_post("TestAO", 5, 3);
        user(1, "Test message");
    }
}
