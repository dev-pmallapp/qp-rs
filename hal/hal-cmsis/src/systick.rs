//! SysTick Timer driver

use hal::mmio::{RO, RW};
use hal::timer::{Timer, TimerMode};
use hal::error::{HalError, HalResult};

const SYSTICK_BASE: usize = 0xE000_E010;

/// Memory-mapped SysTick registers
#[repr(C)]
pub struct SysTickRegs {
    /// Control and Status Register
    pub csr:   RW<u32>,
    /// Reload Value Register
    pub rvr:   RW<u32>,
    /// Current Value Register
    pub cvr:   RW<u32>,
    /// Calibration Value Register
    pub calib: RO<u32>,
}

const CSR_ENABLE:    u32 = 1 << 0;
const CSR_TICKINT:   u32 = 1 << 1;
const CSR_CLKSOURCE: u32 = 1 << 2; // 1 = processor clock

fn systick() -> &'static SysTickRegs {
    unsafe { &*(SYSTICK_BASE as *const SysTickRegs) }
}

/// SysTick Timer implementation
pub struct SysTickTimer {
    core_mhz: u32,
}

impl SysTickTimer {
    /// Create a new SysTick Timer instance
    pub const fn new(core_mhz: u32) -> Self {
        Self { core_mhz }
    }
}

impl Timer for SysTickTimer {
    fn start(&mut self, period_us: u64, _mode: TimerMode) -> HalResult<()> {
        let ticks = ((period_us * self.core_mhz as u64) as u32).saturating_sub(1);
        if ticks == 0 || ticks > 0x00FF_FFFF {
            return Err(HalError::InvalidParameter);
        }
        let st = systick();
        st.csr.write(0);
        st.rvr.write(ticks);
        st.cvr.write(0);
        st.csr.write(CSR_ENABLE | CSR_TICKINT | CSR_CLKSOURCE);
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> {
        systick().csr.write(0);
        Ok(())
    }

    fn counter(&self) -> u64 {
        systick().cvr.read() as u64
    }

    fn enable_interrupt(&mut self) -> HalResult<()> {
        systick().csr.modify(|v| v | CSR_TICKINT);
        Ok(())
    }

    fn disable_interrupt(&mut self) -> HalResult<()> {
        systick().csr.modify(|v| v & !CSR_TICKINT);
        Ok(())
    }

    fn clear_interrupt(&mut self) -> HalResult<()> {
        let _ = systick().csr.read();
        Ok(())
    }
}
