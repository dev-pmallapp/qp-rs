//! Ccompare Timer driver for Xtensa LX

use core::arch::asm;
use hal::error::{HalError, HalResult};
use hal::timer::{Timer, TimerMode};

fn read_ccount() -> u32 {
    #[cfg(target_arch = "xtensa")]
    {
        let v: u32;
        unsafe { core::arch::asm!("rsr.ccount {0}", out(reg) v, options(nomem, nostack)) }
        v
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        0
    }
}

fn read_ccompare0() -> u32 {
    #[cfg(target_arch = "xtensa")]
    {
        let v: u32;
        unsafe { core::arch::asm!("rsr.ccompare0 {0}", out(reg) v, options(nomem, nostack)) }
        v
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        0
    }
}

unsafe fn write_ccompare0(val: u32) {
    #[cfg(target_arch = "xtensa")]
    {
        unsafe { core::arch::asm!("wsr.ccompare0 {0}", in(reg) val, options(nomem, nostack)) }
        unsafe { core::arch::asm!("isync", options(nostack, preserves_flags)) }
    }
    #[cfg(not(target_arch = "xtensa"))]
    {
        let _ = val;
    }
}

/// Ccompare Timer implementation
pub struct CcompareTimer {
    core_mhz:  u32,
    period_cy: u32,   // period in cycles — used for periodic reload
    periodic:  bool,
}

impl CcompareTimer {
    /// Create a new CcompareTimer instance
    pub const fn new(core_mhz: u32) -> Self {
        Self { core_mhz, period_cy: 0, periodic: false }
    }
}

impl Timer for CcompareTimer {
    fn start(&mut self, period_us: u64, mode: TimerMode) -> HalResult<()> {
        let cycles = (period_us * self.core_mhz as u64) as u32;
        if cycles == 0 { return Err(HalError::InvalidParameter); }
        self.period_cy = cycles;
        self.periodic  = matches!(mode, TimerMode::Periodic);
        let next = read_ccount().wrapping_add(cycles);
        unsafe { write_ccompare0(next) }
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> {
        // Writing CCOMPARE0 far ahead effectively disables the timer.
        unsafe { write_ccompare0(read_ccount().wrapping_add(u32::MAX / 2)) }
        Ok(())
    }

    fn counter(&self) -> u64 { read_ccount() as u64 }

    fn enable_interrupt(&mut self)  -> HalResult<()> { Ok(()) }  // enabled by INTENABLE bit 6
    fn disable_interrupt(&mut self) -> HalResult<()> { Ok(()) }  // disabled by INTENABLE bit 6

    fn clear_interrupt(&mut self) -> HalResult<()> {
        // Reload the compare register for the next period (periodic mode).
        if self.periodic {
            let next = read_ccompare0().wrapping_add(self.period_cy);
            unsafe { write_ccompare0(next) }
        }
        Ok(())
    }
}
