//! Core barrier and wait instructions for ARM Cortex-M

use core::arch::asm;

/// Data Synchronization Barrier
#[inline(always)]
pub fn dsb() {
    unsafe { asm!("dsb", options(nostack, preserves_flags)) }
}

/// Instruction Synchronization Barrier
#[inline(always)]
pub fn isb() {
    unsafe { asm!("isb", options(nostack, preserves_flags)) }
}

/// Data Memory Barrier
#[inline(always)]
pub fn dmb() {
    unsafe { asm!("dmb", options(nostack, preserves_flags)) }
}

/// No Operation
#[inline(always)]
pub fn nop() {
    unsafe { asm!("nop", options(nostack, preserves_flags)) }
}

/// Wait For Interrupt
#[inline(always)]
pub fn wfi() {
    unsafe { asm!("wfi", options(nostack, preserves_flags)) }
}

/// Wait For Event
#[inline(always)]
pub fn wfe() {
    unsafe { asm!("wfe", options(nostack, preserves_flags)) }
}
