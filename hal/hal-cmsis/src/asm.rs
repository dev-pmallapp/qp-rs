//! Core barrier and wait instructions for ARM Cortex-M

/// Data Synchronization Barrier
#[inline(always)]
pub fn dsb() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("dsb", options(nostack, preserves_flags)) }
}

/// Instruction Synchronization Barrier
#[inline(always)]
pub fn isb() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("isb", options(nostack, preserves_flags)) }
}

/// Data Memory Barrier
#[inline(always)]
pub fn dmb() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("dmb", options(nostack, preserves_flags)) }
}

/// No Operation
#[inline(always)]
pub fn nop() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("nop", options(nostack, preserves_flags)) }
}

/// Wait For Interrupt
#[inline(always)]
pub fn wfi() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) }
}

/// Wait For Event
#[inline(always)]
pub fn wfe() {
    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("wfe", options(nostack, preserves_flags)) }
}
