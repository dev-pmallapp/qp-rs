//! INTENABLE interrupt controller implementation for Xtensa LX

use core::arch::asm;
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

fn read_intenable() -> u32 {
    #[cfg(target_arch = "xtensa")]
    {
        let v: u32;
        unsafe { core::arch::asm!("rsr.intenable {0}", out(reg) v, options(nomem, nostack)) }
        v
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        0
    }
}

unsafe fn write_intenable(val: u32) {
    #[cfg(target_arch = "xtensa")]
    {
        unsafe { core::arch::asm!("wsr.intenable {0}", in(reg) val, options(nomem, nostack)) }
        unsafe { core::arch::asm!("isync", options(nostack, preserves_flags)) }
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        let _ = val;
    }
}

fn read_interrupt() -> u32 {
    #[cfg(target_arch = "xtensa")]
    {
        let v: u32;
        unsafe { core::arch::asm!("rsr.interrupt {0}", out(reg) v, options(nomem, nostack)) }
        v
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        0
    }
}

#[allow(dead_code)]
unsafe fn write_intset(mask: u32) {
    #[cfg(target_arch = "xtensa")]
    {
        unsafe { core::arch::asm!("wsr.intset {0}", in(reg) mask, options(nomem, nostack)) }
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        let _ = mask;
    }
}

/// Clear a software interrupt.
unsafe fn write_intclear(mask: u32) {
    #[cfg(target_arch = "xtensa")]
    {
        unsafe { core::arch::asm!("wsr.intclear {0}", in(reg) mask, options(nomem, nostack)) }
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        let _ = mask;
    }
}

/// Intenable controller structure
pub struct IntenableController {
    _private: (),
}

impl IntenableController {
    /// Create a new IntenableController handle
    ///
    /// # Safety
    /// This must be constructed at most once per CPU core.
    pub const unsafe fn new() -> Self {
        Self { _private: () }
    }
}

impl InterruptController for IntenableController {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 {
            return Err(HalError::InvalidParameter);
        }
        let cur = read_intenable();
        unsafe { write_intenable(cur | (1 << irq)) }
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 {
            return Err(HalError::InvalidParameter);
        }
        let cur = read_intenable();
        unsafe { write_intenable(cur & !(1 << irq)) }
        Ok(())
    }

    fn set_priority(&mut self, _irq: u32, _priority: InterruptPriority) -> HalResult<()> {
        // Interrupt priority on ESP32 is fixed in hardware per interrupt source.
        // Reassignment requires reprogramming the interrupt matrix via peripheral
        // registers — handled in the BSP, not here.
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq >= 32 {
            return false;
        }
        (read_interrupt() >> irq) & 1 != 0
    }

    fn clear_pending(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 32 {
            return Err(HalError::InvalidParameter);
        }
        unsafe { write_intclear(1 << irq) }
        Ok(())
    }
}
