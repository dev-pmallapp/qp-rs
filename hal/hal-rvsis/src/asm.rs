//! Core barrier and wait instructions for RISC-V

/// Full memory fence — orders all memory operations on all devices.
/// Equivalent to ARM DSB + DMB combined.
#[inline(always)]
pub fn fence() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack, preserves_flags)) }
}

/// Instruction fence — synchronises instruction stream with data memory.
/// Equivalent to ARM ISB.
#[inline(always)]
pub fn fence_i() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe { core::arch::asm!("fence.i", options(nostack, preserves_flags)) }
}

/// No Operation
#[inline(always)]
pub fn nop() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe { core::arch::asm!("nop", options(nostack, preserves_flags)) }
}

/// Wait For Interrupt — stall until an interrupt or event wakes the core.
#[inline(always)]
pub fn wfi() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) }
}
