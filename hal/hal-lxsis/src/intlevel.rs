//! Interrupt level lock implementation for Xtensa LX

use core::arch::asm;

/// Read PS and set PS.INTLEVEL to `level`. Returns the previous PS value.
///
/// Interrupt levels on Xtensa (ESP32): 1–5 = normal, 6 = debug, 7 = NMI.
/// Setting level 5 masks all maskable interrupts.
///
/// # Safety
/// Caller must restore with `wsr_ps(prev)`.
#[inline(always)]
pub unsafe fn rsil(level: u32) -> u32 {
    let ps: u32;
    unsafe { asm!("rsil {0}, {1}", out(reg) ps, in(reg) level, options(nostack)) }
    ps
}

/// Write the Processor State register (restores a saved PS value).
///
/// # Safety
/// Caller must ensure the saved PS value is valid.
#[inline(always)]
pub unsafe fn wsr_ps(ps: u32) {
    unsafe { asm!("wsr.ps {0}", in(reg) ps, options(nostack)) }
    // isync required after WSR.PS before the new level takes effect.
    unsafe { asm!("isync", options(nostack, preserves_flags)) }
}
