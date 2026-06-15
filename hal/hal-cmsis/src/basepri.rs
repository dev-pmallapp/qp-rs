//! BASEPRI register access for ARMv7-M

use core::arch::asm;

/// Read the current BASEPRI register value
#[inline(always)]
pub fn read() -> u8 {
    let v: usize;
    unsafe {
        asm!("mrs {}, BASEPRI", out(reg) v, options(nomem, nostack, preserves_flags))
    }
    v as u8
}

/// Write a new value to the BASEPRI register
///
/// # Safety
/// Caller must ensure that restoring the previous BASEPRI value is handled properly.
#[inline(always)]
pub unsafe fn write(val: u8) {
    let val_usize = val as usize;
    unsafe {
        asm!("msr BASEPRI, {}", in(reg) val_usize, options(nomem, nostack, preserves_flags))
    }
}
