//! QUTest target-side infrastructure: test probe registry and helper macro.
//!
//! QUTest is QP's embedded unit-testing framework.  The host tool injects test
//! probes via `RxCmd::TestProbe` (`QS_RX_TEST_PROBE = 9`).  The target checks
//! for registered probes at marked injection points using [`take_test_probe`],
//! which consumes and returns the probe data if one was registered.
//!
//! ## Typical usage in production code
//!
//! ```rust,ignore
//! use qs::records::infra::TEST_PROBE as QS_TEST_PROBE;
//! use qs::qutest::{take_test_probe, make_probe_record};
//!
//! fn my_handler(ctx: &mut ActiveContext) {
//!     let fn_handle = MY_FN_DICT_HANDLE;
//!     if let Some(tp) = take_test_probe(fn_handle) {
//!         // Emit QS_TEST_PROBE_GET (record 59) so QSpy can log it
//!         let _ = ctx.emit_trace(QS_TEST_PROBE, &make_probe_record(fn_handle, tp));
//!         if tp != 0 {
//!             return;  // test-controlled early exit
//!         }
//!     }
//!     // Normal production code
//! }
//! ```
//!
//! Or with the ergonomic macro (the probe data is bound to `qs_tp_`):
//!
//! ```rust,ignore
//! qs_test_probe!(MY_FN_HANDLE => {
//!     if qs_tp_ != 0 { return; }
//! });
//! ```

#[cfg(feature = "std")]
use std::sync::Mutex;
#[cfg(not(feature = "std"))]
use spin::Mutex;

/// Maximum number of simultaneously active test probes (mirrors QP/C++ default of 8).
pub const MAX_TEST_PROBES: usize = 8;

#[derive(Clone, Copy)]
struct Probe {
    fn_ptr: u64,
    data:   u32,
}

struct ProbeRegistry {
    slots: [Option<Probe>; MAX_TEST_PROBES],
}

impl ProbeRegistry {
    const fn new() -> Self {
        Self { slots: [None; MAX_TEST_PROBES] }
    }

    fn set(&mut self, fn_ptr: u64, data: u32) {
        // Overwrite existing slot for the same fn_ptr
        for p in self.slots.iter_mut().flatten() {
            if p.fn_ptr == fn_ptr {
                p.data = data;
                return;
            }
        }
        // Find an empty slot
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some(Probe { fn_ptr, data });
                return;
            }
        }
        // All slots full — silently overwrite slot 0
        self.slots[0] = Some(Probe { fn_ptr, data });
    }

    fn take(&mut self, fn_ptr: u64) -> Option<u32> {
        for slot in &mut self.slots {
            if let Some(p) = slot {
                if p.fn_ptr == fn_ptr {
                    let data = p.data;
                    *slot = None;
                    return Some(data);
                }
            }
        }
        None
    }

    fn clear(&mut self) {
        self.slots = [None; MAX_TEST_PROBES];
    }
}

static REGISTRY: Mutex<ProbeRegistry> = Mutex::new(ProbeRegistry::new());

/// Register (or overwrite) a test probe for `fn_ptr`.
///
/// Called when `RxCmd::TestProbe` arrives from the host tool.  The next call
/// to [`take_test_probe`] with the same `fn_ptr` will return `data`.
pub fn set_test_probe(fn_ptr: u64, data: u32) {
    #[cfg(feature = "std")]
    REGISTRY.lock().unwrap().set(fn_ptr, data);
    #[cfg(not(feature = "std"))]
    REGISTRY.lock().set(fn_ptr, data);
}

/// Remove and return the probe data for `fn_ptr`, if one is registered.
///
/// The probe is **consumed** on the first call — subsequent calls for the same
/// function pointer return `None` until the host registers a new probe.
pub fn take_test_probe(fn_ptr: u64) -> Option<u32> {
    #[cfg(feature = "std")]
    { REGISTRY.lock().unwrap().take(fn_ptr) }
    #[cfg(not(feature = "std"))]
    { REGISTRY.lock().take(fn_ptr) }
}

/// Clear all registered test probes.
///
/// Called when `RxCmd::TestSetup` or `RxCmd::TestTeardown` arrives.
pub fn clear_test_probes() {
    #[cfg(feature = "std")]
    REGISTRY.lock().unwrap().clear();
    #[cfg(not(feature = "std"))]
    REGISTRY.lock().clear();
}

/// Build the `QS_TEST_PROBE_GET` (record 59) payload.
///
/// Format: `[fn_ptr: u64 LE] [data: u32 LE]` — 12 bytes total.
/// Emit this payload with record type [`crate::records::infra::TEST_PROBE`]
/// whenever a probe fires, so QSpy can display `TstProbe Fun=…,Data=…`.
pub fn make_probe_record(fn_ptr: u64, data: u32) -> [u8; 12] {
    let mut buf = [0u8; 12];
    buf[0..8].copy_from_slice(&fn_ptr.to_le_bytes());
    buf[8..12].copy_from_slice(&data.to_le_bytes());
    buf
}

/// Run `$code` if a test probe is registered for `$fn_ptr`.
///
/// Inside `$code`, the identifier `qs_tp_` holds the probe data (`u32`).
///
/// # Example
/// ```rust,ignore
/// qs_test_probe!(MY_FN_HANDLE => {
///     if qs_tp_ != 0 { return; }
/// });
/// ```
#[macro_export]
macro_rules! qs_test_probe {
    ($fn_ptr:expr => $code:block) => {
        #[allow(unused_variables)]
        if let Some(qs_tp_) = $crate::qutest::take_test_probe($fn_ptr as u64) {
            $code
        }
    };
    ($fn_ptr:expr, |$tp:ident| $code:block) => {
        if let Some($tp) = $crate::qutest::take_test_probe($fn_ptr as u64) {
            $code
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(f: impl FnOnce()) {
        clear_test_probes();
        f();
        clear_test_probes();
    }

    #[test]
    fn probe_set_and_take() {
        clean(|| {
            set_test_probe(0xDEAD_0001, 42);
            assert_eq!(take_test_probe(0xDEAD_0001), Some(42));
            // Consumed — second call returns None
            assert_eq!(take_test_probe(0xDEAD_0001), None);
        });
    }

    #[test]
    fn probe_overwrite() {
        clean(|| {
            set_test_probe(0xDEAD_0002, 1);
            set_test_probe(0xDEAD_0002, 2);
            assert_eq!(take_test_probe(0xDEAD_0002), Some(2));
        });
    }

    #[test]
    fn probe_clear() {
        clean(|| {
            set_test_probe(0xDEAD_0003, 99);
            clear_test_probes();
            assert_eq!(take_test_probe(0xDEAD_0003), None);
        });
    }

    #[test]
    fn multiple_independent_probes() {
        clean(|| {
            for i in 0..MAX_TEST_PROBES {
                set_test_probe(0xBEEF_0000 + i as u64, i as u32 * 10);
            }
            for i in 0..MAX_TEST_PROBES {
                assert_eq!(take_test_probe(0xBEEF_0000 + i as u64), Some(i as u32 * 10));
            }
        });
    }

    #[test]
    fn unknown_fn_ptr_returns_none() {
        clean(|| {
            assert_eq!(take_test_probe(0xFFFF_FFFF_DEAD_BEEF), None);
        });
    }

    #[test]
    fn make_probe_record_encodes_correctly() {
        let fn_ptr = 0xCAFE_BABE_DEAD_BEEF_u64;
        let data   = 0x1234_5678_u32;
        let rec = make_probe_record(fn_ptr, data);
        assert_eq!(u64::from_le_bytes(rec[0..8].try_into().unwrap()), fn_ptr);
        assert_eq!(u32::from_le_bytes(rec[8..12].try_into().unwrap()), data);
    }
}
