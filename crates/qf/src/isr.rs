//! ISR nesting counter and ISR-context utilities (Phase 5).
//!
//! In a QK preemptive kernel, every interrupt service routine (ISR) must
//! bracket its body with `qk_isr_entry!()` / `qk_isr_exit!()`.  The
//! framework uses the nesting counter to:
//!
//! - Distinguish task-level from ISR-level context for assertions.
//! - Defer scheduler activation until the outermost ISR returns.
//! - Gate QS trace records that are only valid in task context.
//!
//! # Usage
//!
//! ```rust,ignore
//! #[interrupt]
//! fn TIMER0() {
//!     qf::qk_isr_entry!();
//!
//!     kernel.tick_from_isr().ok();
//!     kernel.run_until_idle();  // activates scheduler on outermost exit
//!
//!     qf::qk_isr_exit!();
//! }
//! ```
//!
//! # Thread safety
//!
//! The counter is stored in an `AtomicU8` and is safe to read/write from
//! multiple interrupt priorities without a mutex.

use core::sync::atomic::{AtomicU8, Ordering};

/// Global ISR nesting depth counter.
///
/// `0` means task-level context; `>0` means ISR context.
pub static ISR_NESTING: AtomicU8 = AtomicU8::new(0);

/// Returns the current ISR nesting depth.
#[inline]
pub fn isr_nesting() -> u8 {
    ISR_NESTING.load(Ordering::Relaxed)
}

/// Returns `true` when the caller is executing inside an ISR.
#[inline]
pub fn in_isr() -> bool {
    ISR_NESTING.load(Ordering::Relaxed) > 0
}

/// Increment the ISR nesting counter.
///
/// # Safety
///
/// Must be called exactly once at the **beginning** of every ISR, before
/// any QP framework calls. Corresponds to `QK_ISR_ENTRY()` in QP/C++.
#[inline]
pub unsafe fn isr_enter() {
    ISR_NESTING.fetch_add(1, Ordering::AcqRel);
}

/// Decrement the ISR nesting counter.
///
/// # Safety
///
/// Must be called exactly once at the **end** of every ISR, after all QP
/// framework calls. When this call reduces the nesting depth to zero, it
/// is safe to run the scheduler again.
#[inline]
pub unsafe fn isr_exit() {
    let prev = ISR_NESTING.fetch_sub(1, Ordering::AcqRel);
    debug_assert!(prev > 0, "qk_isr_exit!: nesting counter underflow");
}
