//! Core-Local Interruptor (CLINT) Timer driver for RISC-V

use hal::error::{HalError, HalResult};
use hal::timer::{Timer, TimerMode};

/// Default CLINT base address
pub const CLINT_BASE_DEFAULT: usize = 0x0200_0000;

const MTIME_OFFSET:    usize = 0xBFF8;
const MTIMECMP_OFFSET: usize = 0x4000;

/// CLINT Timer implementation
pub struct ClintTimer {
    base:      usize,
    hz:        u64,    // mtime frequency (Hz)
    period_cy: u64,
    periodic:  bool,
}

impl ClintTimer {
    /// Create a new ClintTimer instance
    ///
    /// # Safety
    /// `base` must be the CLINT base address for this target;
    /// `hz` is the clock frequency of the mtime counter.
    pub const fn new(base: usize, hz: u64) -> Self {
        Self { base, hz, period_cy: 0, periodic: false }
    }

    fn mtime(&self) -> u64 {
        let lo = unsafe { core::ptr::read_volatile((self.base + MTIME_OFFSET)     as *const u32) };
        let hi = unsafe { core::ptr::read_volatile((self.base + MTIME_OFFSET + 4) as *const u32) };
        (hi as u64) << 32 | lo as u64
    }

    fn set_mtimecmp(&self, val: u64) {
        // Write hi first with MAX to avoid spurious interrupt, then lo, then hi.
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET + 4) as *mut u32, u32::MAX) }
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET)     as *mut u32, val as u32) }
        unsafe { core::ptr::write_volatile((self.base + MTIMECMP_OFFSET + 4) as *mut u32, (val >> 32) as u32) }
    }
}

impl Timer for ClintTimer {
    fn start(&mut self, period_us: u64, mode: TimerMode) -> HalResult<()> {
        let cycles = period_us * self.hz / 1_000_000;
        if cycles == 0 {
            return Err(HalError::InvalidParameter);
        }
        self.period_cy = cycles;
        self.periodic  = matches!(mode, TimerMode::Periodic);
        self.set_mtimecmp(self.mtime() + cycles);
        Ok(())
    }

    fn stop(&mut self) -> HalResult<()> {
        self.set_mtimecmp(u64::MAX);
        Ok(())
    }

    fn counter(&self) -> u64 {
        self.mtime()
    }

    fn enable_interrupt(&mut self) -> HalResult<()> {
        Ok(()) // controlled via MIE.MTIE CSR
    }

    fn disable_interrupt(&mut self) -> HalResult<()> {
        Ok(())
    }

    fn clear_interrupt(&mut self) -> HalResult<()> {
        if self.periodic {
            self.set_mtimecmp(self.mtime() + self.period_cy);
        }
        Ok(())
    }
}
