//! CSR read/write helpers for RISC-V

use core::arch::asm;

pub const MSTATUS: u32 = 0x300;
pub const MIE:     u32 = 0x304;
pub const MIP:     u32 = 0x344;
pub const MCAUSE:  u32 = 0x342;
pub const MEPC:    u32 = 0x341;
pub const MTVEC:   u32 = 0x305;

/// Read a CSR.
#[inline(always)]
pub fn csrr<const CSR: u32>() -> u32 {
    let val: u32;
    unsafe { asm!("csrr {0}, {1}", out(reg) val, const CSR, options(nomem, nostack)) }
    val
}

/// Write a CSR.
///
/// # Safety
/// Writing CSRs changes machine-mode privilege state.
#[inline(always)]
pub unsafe fn csrw<const CSR: u32>(val: u32) {
    unsafe { asm!("csrw {0}, {1}", const CSR, in(reg) val, options(nomem, nostack)) }
}

/// Atomic read-and-set bits in a CSR.
///
/// # Safety
/// Writing CSRs changes machine-mode privilege state.
#[inline(always)]
pub unsafe fn csrrs<const CSR: u32>(mask: u32) -> u32 {
    let old: u32;
    unsafe { asm!("csrrs {0}, {1}, {2}", out(reg) old, const CSR, in(reg) mask, options(nomem, nostack)) }
    old
}

/// Atomic read-and-clear bits in a CSR.
///
/// # Safety
/// Writing CSRs changes machine-mode privilege state.
#[inline(always)]
pub unsafe fn csrrc<const CSR: u32>(mask: u32) -> u32 {
    let old: u32;
    unsafe { asm!("csrrc {0}, {1}, {2}", out(reg) old, const CSR, in(reg) mask, options(nomem, nostack)) }
    old
}

/// Macro for reading a CSR
#[macro_export]
macro_rules! csrr {
    ($csr:expr) => {
        $crate::csr::csrr::<$csr>()
    };
}

/// Macro for writing a CSR
#[macro_export]
macro_rules! csrw {
    ($csr:expr, $val:expr) => {
        $crate::csr::csrw::<$csr>($val)
    };
}

/// Macro for setting bits in a CSR
#[macro_export]
macro_rules! csrrs {
    ($csr:expr, $mask:expr) => {
        $crate::csr::csrrs::<$csr>($mask)
    };
}

/// Macro for clearing bits in a CSR
#[macro_export]
macro_rules! csrrc {
    ($csr:expr, $mask:expr) => {
        $crate::csr::csrrc::<$csr>($mask)
    };
}
