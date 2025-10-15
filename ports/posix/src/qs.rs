//! QS (Quantum Spy) Tracing for POSIX Port
//!
//! Platform-specific QS implementation that integrates with qp-qs crate
//! and provides convenient helper functions for dictionary generation.

#[cfg(feature = "qs")]
use qp_qs as qs;

#[cfg(feature = "qs")]
use std::mem::transmute;

/// QS Dictionary record types (for transmute when qs feature is enabled)
/// Based on QP/C 8.1.1 enum QS_GlbPre
#[cfg(feature = "qs")]
mod dict_types {
    pub const QS_ENUM_DICT: u8 = 54;
    pub const QS_SIG_DICT: u8 = 60;
    pub const QS_OBJ_DICT: u8 = 61;
    pub const QS_FUN_DICT: u8 = 62;
    pub const QS_USR_DICT: u8 = 63;
}

/// Initialize QS tracing
#[cfg(feature = "qs")]
pub fn init() {
    // QS is initialized via init_tcp/init_udp in application code
    println!("QS: Tracing ready (POSIX port)");
}

#[cfg(not(feature = "qs"))]
pub fn init() {
    println!("QS: Tracing disabled (compile with --features qs to enable)");
}

/// Initialize QS with TCP connection to QSPY
#[cfg(feature = "qs")]
pub fn init_tcp(host: &str, port: u16) -> std::io::Result<()> {
    qs::init_tcp(host, port)?;
    qs::enable();
    Ok(())
}

#[cfg(not(feature = "qs"))]
pub fn init_tcp(_host: &str, _port: u16) -> std::io::Result<()> {
    Ok(())
}

/// Initialize QS with UDP connection to QSPY
#[cfg(feature = "qs")]
pub fn init_udp(host: &str, port: u16) -> std::io::Result<()> {
    qs::init_udp(host, port)?;
    qs::enable();
    Ok(())
}

#[cfg(not(feature = "qs"))]
pub fn init_udp(_host: &str, _port: u16) -> std::io::Result<()> {
    Ok(())
}

/// Send TARGET_INFO record
#[cfg(feature = "qs")]
pub fn send_target_info(qp_version: &str, target_name: &str) {
    qs::target_info(
        qp_version,
        target_name,
        if cfg!(target_endian = "little") { 0 } else { 1 }
    );
    qs::flush().ok();
}

#[cfg(not(feature = "qs"))]
pub fn send_target_info(_qp_version: &str, _target_name: &str) {}

/// Send signal dictionary entry
///
/// # Arguments
/// * `signal` - Signal value (typically from an enum as u32)
/// Send signal dictionary entry
///
/// # Arguments
/// * `signal` - The signal number (u16, matching QP/C QSignal)
/// * `obj` - Object pointer to associate with this signal (for scoping)
/// * `name` - Signal name string
///
/// # Example
/// ```ignore
/// send_sig_dict(MySignal::Timeout as u16, &my_obj as *const _ as usize, "TIMEOUT_SIG");
/// ```
#[cfg(feature = "qs")]
pub fn send_sig_dict(signal: u16, obj: usize, name: &str) {
    let sig_dict_type = unsafe { transmute::<u8, _>(dict_types::QS_SIG_DICT) };
    if qs::begin(sig_dict_type, 0) {
        qs::u16(signal);
        qs::obj_ptr(obj);
        qs::str(name);
        qs::end();
    }
}

#[cfg(not(feature = "qs"))]
pub fn send_sig_dict(_signal: u16, _obj: usize, _name: &str) {}

/// Send object dictionary entry
#[cfg(feature = "qs")]
pub fn send_obj_dict(obj_ptr: usize, name: &str) {
    // QS_OBJ_DICT = 70
    let obj_dict_type = unsafe { transmute::<u8, _>(dict_types::QS_OBJ_DICT) };
    if qs::begin(obj_dict_type, 0) {
        qs::u32(obj_ptr as u32); // Use lower 32 bits for compatibility
        qs::str(name);
        qs::end();
    }
}

#[cfg(not(feature = "qs"))]
pub fn send_obj_dict(_obj_ptr: usize, _name: &str) {}

/// Flush all pending QS records
#[cfg(feature = "qs")]
pub fn flush() -> std::io::Result<()> {
    qs::flush()
}

#[cfg(not(feature = "qs"))]
pub fn flush() -> std::io::Result<()> {
    Ok(())
}

/// Re-export QS primitives when feature is enabled
#[cfg(feature = "qs")]
pub use qs::{begin, end, u8, u16, u32, str};

/// Stub implementations when QS is disabled
#[cfg(not(feature = "qs"))]
pub mod stubs {
    pub fn begin<T>(_record_type: T, _qs_id: u8) -> bool { false }
    pub fn end() {}
    pub fn u8(_val: u8) {}
    pub fn u16(_val: u16) {}
    pub fn u32(_val: u32) {}
    pub fn str(_s: &str) {}
}

#[cfg(not(feature = "qs"))]
pub use stubs::*;

/// Helper macro for bulk signal dictionary transmission
///
/// # Example
/// ```ignore
/// send_signal_dict![
///     (MySignal::Timeout, "TIMEOUT_SIG"),
///     (MySignal::Start, "START_SIG"),
///     (MySignal::Stop, "STOP_SIG"),
/// ];
/// ```
#[macro_export]
macro_rules! send_signal_dict {
    [$(($signal:expr, $name:expr)),* $(,)?] => {
        $(
            $crate::qs::send_sig_dict($signal as u32, $name);
        )*
        $crate::qs::flush().ok();
    };
}
