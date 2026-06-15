//! Xtensa interrupt level lock configuration for QK scheduler

#[cfg(feature = "hw")]
use hal_lxsis::intlevel::{rsil, wsr_ps};
#[cfg(feature = "hw")]
use hal_lxsis::asm;

/// Interrupt level for the QK scheduler lock.
///
/// ISRs at level <= QK_INTLEVEL are masked while the scheduler runs.
/// ISRs at level > QK_INTLEVEL (higher urgency) are never masked.
pub const QK_INTLEVEL: u32 = 3;

/// Lock the QK scheduler — disable interrupts below the ceiling level.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_lock() -> u32 {
    let prev = unsafe { rsil(QK_INTLEVEL) };
    asm::memw();
    prev
}

/// Unlock the QK scheduler — restore the previous interrupt level.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_unlock(prev: u32) {
    asm::memw();
    unsafe { wsr_ps(prev) }
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
