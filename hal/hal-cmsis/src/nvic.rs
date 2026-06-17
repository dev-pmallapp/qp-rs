//! NVIC (Nested Vectored Interrupt Controller) driver

use hal::mmio::{RO, RW};
use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

const NVIC_BASE: usize = 0xE000_E100;

/// Memory-mapped NVIC registers
#[repr(C)]
pub struct NvicRegs {
    /// Interrupt Set-Enable Registers
    pub iser: [RW<u32>; 8],   // 0x000
    _r0:      [u32; 24],
    /// Interrupt Clear-Enable Registers
    pub icer: [RW<u32>; 8],   // 0x080
    _r1:      [u32; 24],
    /// Interrupt Set-Pending Registers
    pub ispr: [RW<u32>; 8],   // 0x100
    _r2:      [u32; 24],
    /// Interrupt Clear-Pending Registers
    pub icpr: [RW<u32>; 8],   // 0x180
    _r3:      [u32; 24],
    /// Interrupt Active Bit Registers
    pub iabr: [RO<u32>; 8],   // 0x200
    _r4:      [u32; 56],
    /// Interrupt Priority Registers
    pub ipr:  [RW<u8>; 240],  // 0x300
}

fn nvic() -> &'static NvicRegs {
    unsafe { &*(NVIC_BASE as *const NvicRegs) }
}

/// NVIC controller implementation
pub struct NvicController {
    _private: (),
}

impl NvicController {
    /// Create a new NVIC controller handle
    ///
    /// # Safety
    /// This must be constructed at most once per CPU core.
    pub const unsafe fn new() -> Self {
        Self { _private: () }
    }
}

impl InterruptController for NvicController {
    fn enable_interrupt(&mut self, irq_num: u32) -> HalResult<()> {
        if irq_num >= 240 {
            return Err(HalError::InvalidParameter);
        }
        nvic().iser[(irq_num / 32) as usize].write(1 << (irq_num % 32));
        Ok(())
    }

    fn disable_interrupt(&mut self, irq_num: u32) -> HalResult<()> {
        if irq_num >= 240 {
            return Err(HalError::InvalidParameter);
        }
        nvic().icer[(irq_num / 32) as usize].write(1 << (irq_num % 32));
        crate::asm::dsb();
        crate::asm::isb();
        Ok(())
    }

    fn set_priority(&mut self, irq_num: u32, priority: InterruptPriority) -> HalResult<()> {
        if irq_num >= 240 {
            return Err(HalError::InvalidParameter);
        }
        nvic().ipr[irq_num as usize].write(priority);
        Ok(())
    }

    fn is_pending(&self, irq_num: u32) -> bool {
        if irq_num >= 240 {
            return false;
        }
        (nvic().ispr[(irq_num / 32) as usize].read() >> (irq_num % 32)) & 1 != 0
    }

    fn clear_pending(&mut self, irq_num: u32) -> HalResult<()> {
        if irq_num >= 240 {
            return Err(HalError::InvalidParameter);
        }
        nvic().icpr[(irq_num / 32) as usize].write(1 << (irq_num % 32));
        Ok(())
    }
}
