//! ESP32-C3 Interrupt Matrix routing and priority control

use hal::error::{HalError, HalResult};
use hal::interrupt::{InterruptController, InterruptPriority};

/// ESP32-C3 Interrupt Matrix controller
pub struct Esp32C3IntMatrix {
    _private: (),
}

impl Esp32C3IntMatrix {
    /// Create a new Esp32C3IntMatrix handle
    ///
    /// # Safety
    /// This must be constructed at most once per CPU core.
    pub const unsafe fn new() -> Self {
        Self { _private: () }
    }

    fn map_reg(&self, irq: u32) -> *mut u32 {
        (0x600C_2000 + 4 * irq as usize) as *mut u32
    }

    fn pri_reg(&self, cpu_int: u32) -> *mut u32 {
        (0x600C_2000 + 0x0118 + 4 * (cpu_int - 1) as usize) as *mut u32
    }
}

impl InterruptController for Esp32C3IntMatrix {
    fn enable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 62 {
            return Err(HalError::InvalidParameter);
        }
        let cpu_int = (irq % 31) + 1;
        unsafe {
            core::ptr::write_volatile(self.map_reg(irq), cpu_int);
        }
        Ok(())
    }

    fn disable_interrupt(&mut self, irq: u32) -> HalResult<()> {
        if irq >= 62 {
            return Err(HalError::InvalidParameter);
        }
        unsafe {
            core::ptr::write_volatile(self.map_reg(irq), 0); // 0 disables routing
        }
        Ok(())
    }

    fn set_priority(&mut self, irq: u32, priority: InterruptPriority) -> HalResult<()> {
        if irq >= 62 {
            return Err(HalError::InvalidParameter);
        }
        let cpu_int = (irq % 31) + 1;
        unsafe {
            core::ptr::write_volatile(self.pri_reg(cpu_int), priority as u32);
        }
        Ok(())
    }

    fn is_pending(&self, irq: u32) -> bool {
        if irq >= 62 {
            return false;
        }
        let cpu_int = (irq % 31) + 1;
        let mip = crate::csr::csrr::<{crate::csr::MIP}>();
        (mip >> cpu_int) & 1 != 0
    }

    fn clear_pending(&mut self, irq: u32) -> HalResult<()> {
        let _ = irq;
        Ok(())
    }
}
