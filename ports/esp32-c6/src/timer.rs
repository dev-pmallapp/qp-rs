use core::sync::atomic::{AtomicU32, Ordering};

/// Placeholder for the ESP32-C6 system timer driving QK ticks.
#[derive(Debug)]
pub struct SystemTimer {
    tick_hz: AtomicU32,
}

impl SystemTimer {
    /// Creates a timer without an active periodic configuration.
    pub const fn new() -> Self {
        Self {
            tick_hz: AtomicU32::new(0),
        }
    }

    /// Configures the periodic tick frequency expected by the kernel.
    pub fn configure_periodic(&self, tick_hz: u32) {
        self.tick_hz.store(tick_hz, Ordering::Release);
        #[cfg(feature = "rt")]
        {
            use hal_rvsis::clint::ClintTimer;
            use hal::timer::{Timer, TimerMode};
            // ESP32-C6 XTAL clock is 40 MHz; CLINT base is 0x2000_0000 in Renode
            let period_us = 1_000_000u64 / tick_hz as u64;
            let mut clint = ClintTimer::new(0x2000_0000, 40_000_000);
            let _ = clint.start(period_us, TimerMode::Periodic);
        }
    }

    /// Returns the currently configured tick frequency in hertz.
    pub fn tick_hz(&self) -> u32 {
        self.tick_hz.load(Ordering::Acquire)
    }
}
