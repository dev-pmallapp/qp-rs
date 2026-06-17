//! NVIC scheduler lock configuration for ARM Cortex-M

#[cfg(feature = "hw")]
use hal_cmsis::basepri;
#[cfg(feature = "hw")]
use hal_cmsis::asm;

/// Priority ceiling for the QK scheduler lock.
///
/// ISRs with numerical priority < QK_BASEPRI are never masked.
/// ISRs at >= QK_BASEPRI are masked during the scheduler critical section
/// and are the only ISRs permitted to call post_from_isr().
pub const QK_BASEPRI: u8 = 0x50;

/// Lock the QK scheduler — disable interrupts below the ceiling priority.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_lock() -> u8 {
    let prev = basepri::read();
    unsafe { basepri::write(QK_BASEPRI) }
    asm::dsb();
    asm::isb();
    prev
}

/// Unlock the QK scheduler — restore the previous priority ceiling.
#[cfg(feature = "hw")]
#[inline]
pub fn qk_unlock(prev: u8) {
    unsafe { basepri::write(prev) }
    asm::dsb();
    asm::isb();
}

/// Stub implementation for non-hardware (host) builds.
#[cfg(not(feature = "hw"))]
#[inline]
pub fn qk_lock() -> u8 {
    0
}

/// Stub implementation for non-hardware (host) builds.
#[cfg(not(feature = "hw"))]
#[inline]
pub fn qk_unlock(_prev: u8) {}
