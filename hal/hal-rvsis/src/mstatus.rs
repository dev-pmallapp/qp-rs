//! Interrupt context locking/unlocking for RISC-V M-mode

use crate::csr::{csrrc, csrrs, MSTATUS};
use crate::asm;

const MSTATUS_MIE: u32 = 1 << 3;   // Machine Interrupt Enable

/// Lock the QK scheduler — disable all M-mode interrupts.
/// Returns the previous MSTATUS value for restore.
#[inline(always)]
pub fn qk_lock() -> u32 {
    // csrrc atomically reads MSTATUS and clears MIE.
    let prev = unsafe { csrrc::<MSTATUS>(MSTATUS_MIE) };
    asm::fence();
    prev
}

/// Unlock the QK scheduler — restore MSTATUS to its previous value.
#[inline(always)]
pub fn qk_unlock(prev: u32) {
    asm::fence();
    if prev & MSTATUS_MIE != 0 {
        unsafe { csrrs::<MSTATUS>(MSTATUS_MIE) };
    }
}
