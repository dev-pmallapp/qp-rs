//! Core barrier and wait instructions for RISC-V

use core::arch::asm;

/// Full memory fence — orders all memory operations on all devices.
/// Equivalent to ARM DSB + DMB combined.
#[inline(always)]
pub fn fence() {
    unsafe { asm!("fence iorw, iorw", options(nostack, preserves_flags)) }
}

/// Instruction fence — synchronises instruction stream with data memory.
/// Equivalent to ARM ISB.
#[inline(always)]
pub fn fence_i() {
    unsafe { asm!("fence.i", options(nostack, preserves_flags)) }
}

/// No Operation
#[inline(always)]
pub fn nop() {
    unsafe { asm!("nop", options(nostack, preserves_flags)) }
}

/// Wait For Interrupt — stall until an interrupt or event wakes the core.
#[inline(always)]
pub fn wfi() {
    unsafe { asm!("wfi", options(nostack, preserves_flags)) }
}
