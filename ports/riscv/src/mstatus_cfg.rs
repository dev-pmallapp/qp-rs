//! RISC-V mstatus scheduler lock configuration for QK scheduler

#[cfg(feature = "hw")]
use hal_rvsis::csr::{csrrc, csrrs, MSTATUS};
#[cfg(feature = "hw")]
use hal_rvsis::asm;

#[cfg(feature = "hw")]
const MSTATUS_MIE: u32 = 1 << 3;   // Machine Interrupt Enable

/// Lock the QK scheduler — disable all M-mode interrupts.
/// Returns the previous MSTATUS value for restore.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_lock() -> u32 {
    let prev = unsafe { csrrc::<MSTATUS>(MSTATUS_MIE) };
    asm::fence();
    prev
}

/// Unlock the QK scheduler — restore MSTATUS to its previous value.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_unlock(prev: u32) {
    asm::fence();
    if prev & MSTATUS_MIE != 0 {
        unsafe { csrrs::<MSTATUS>(MSTATUS_MIE) };
    }
}

/// Stub implementation for non-hardware (host) builds.
#[cfg(not(feature = "hw"))]
#[inline]
pub fn qk_lock() -> u32 {
    0
}

/// Stub implementation for non-hardware (host) builds.
#[cfg(not(feature = "hw"))]
#[inline]
pub fn qk_unlock(_prev: u32) {}
