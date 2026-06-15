//! Core barrier and wait instructions for Xtensa LX

/// Memory Wait — ensures all outstanding memory operations complete.
/// Equivalent to ARM DSB for load/store ordering.
#[inline(always)]
pub fn memw() {
    #[cfg(target_arch = "xtensa")]
    unsafe { core::arch::asm!("memw", options(nostack, preserves_flags)) }
}

/// Instruction Sync — flushes the instruction pipeline.
/// Equivalent to ARM ISB.
#[inline(always)]
pub fn isync() {
    #[cfg(target_arch = "xtensa")]
    unsafe { core::arch::asm!("isync", options(nostack, preserves_flags)) }
}

/// No Operation
#[inline(always)]
pub fn nop() {
    #[cfg(target_arch = "xtensa")]
    unsafe { core::arch::asm!("nop", options(nostack, preserves_flags)) }
}

/// Wait For Interrupt — core enters low-power state until an interrupt fires.
#[inline(always)]
pub fn waiti(level: u32) {
    // WAITI sets PS.INTLEVEL to `level` and halts until an interrupt arrives.
    #[cfg(target_arch = "xtensa")]
    unsafe { core::arch::asm!("waiti {0}", in(reg) level, options(nostack)) }
    #[cfg(not(target_arch = "xtensa"))]
    let _ = level;
}
