//! Platform-Level Interrupt Controller (PLIC) driver for RISC-V

use hal::mmio::{RO, RW};
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

/// Standard RISC-V PLIC layout base address
pub const PLIC_BASE_DEFAULT: usize = 0x0C00_0000;

/// Memory-mapped PLIC registers structure (informative representation)
#[repr(C)]
pub struct PlicRegs {
    /// Source priority (0 = disabled)
    pub priority:  [RW<u32>; 1024],  // 0x000000
    /// Interrupt pending bits
    pub pending:   [RO<u32>; 32],    // 0x001000
    _r0:           [u32; 992],
    /// Enable bits per context
    pub enable:    [[RW<u32>; 32]; 15872], // 0x002000
    _r1:           [u32; 0x1F_C000 / 4],
}

/// PLIC Controller handle
pub struct PlicController {
    base:    usize,
    context: usize,   // Hart context index (0 = hart 0 M-mode)
}

impl PlicController {
    /// Create a new PlicController instance
    ///
    /// # Safety
    /// `base` must be the PLIC base address for this target;
    /// `context` must be the RISC-V hart context index.
    pub const unsafe fn new(base: usize, context: usize) -> Self {
        Self { base, context }
    }

    fn priority_reg(&self, irq: u32) -> *mut u32 {
        (self.base + 4 * irq as usize) as *mut u32
    }

    fn enable_reg(&self, irq: u32) -> *mut u32 {
        (self.base + 0x2000 + self.context * 0x80 + 4 * (irq / 32) as usize) as *mut u32
    }

    #[allow(dead_code)]
    fn threshold_reg(&self) -> *mut u32 {
        (self.base + 0x20_0000 + self.context * 0x1000) as *mut u32
    }

    fn claim_reg(&self) -> *mut u32 {
        (self.base + 0x20_0004 + self.context * 0x1000) as *mut u32
    }

    /// Claim the highest-priority pending interrupt. Returns the IRQ number.
    pub fn claim(&self) -> u32 {
        unsafe { core::ptr::read_volatile(self.claim_reg()) }
    }

    /// Signal completion of the claimed IRQ.
    pub fn complete(&self, irq: u32) {
        unsafe { core::ptr::write_volatile(self.claim_reg(), irq) }
    }
}

impl InterruptController for PlicController {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq == 0 || irq >= 1024 {
            return Err(HalError::InvalidParameter);
        }
        let reg = self.enable_reg(irq);
        let cur = unsafe { core::ptr::read_volatile(reg) };
        unsafe { core::ptr::write_volatile(reg, cur | (1 << (irq % 32))) }
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq == 0 || irq >= 1024 {
            return Err(HalError::InvalidParameter);
        }
        let reg = self.enable_reg(irq);
        let cur = unsafe { core::ptr::read_volatile(reg) };
        unsafe { core::ptr::write_volatile(reg, cur & !(1 << (irq % 32))) }
        Ok(())
    }

    fn set_priority(&mut self, irq: u32, priority: InterruptPriority) -> HalResult<()> {
        if irq == 0 || irq >= 1024 {
            return Err(HalError::InvalidParameter);
        }
        unsafe { core::ptr::write_volatile(self.priority_reg(irq), priority as u32) }
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq == 0 || irq >= 1024 {
            return false;
        }
        let reg = (self.base + 0x1000 + 4 * (irq / 32) as usize) as *const u32;
        (unsafe { core::ptr::read_volatile(reg) } >> (irq % 32)) & 1 != 0
    }

    fn clear_pending(&mut self, _irq: u32) -> HalResult<()> {
        // PLIC clears pending via claim/complete cycle — not a direct write.
        Ok(())
    }
}
