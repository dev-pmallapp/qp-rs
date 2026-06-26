use core::sync::atomic::{AtomicU32, Ordering};

/// Placeholder for the ESP32-S3 system timer driving QK ticks.
#[derive(Debug)]
pub struct SystemTimer {
    tick_hz: AtomicU32,
}

impl Default for SystemTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemTimer {
    /// Creates a timer without an active periodic configuration.
    pub const fn new() -> Self {
        Self {
            tick_hz: AtomicU32::new(0),
        }
    }

    /// Configures the periodic tick frequency via the Xtensa CCOMPARE0 timer.
    pub fn configure_periodic(&self, tick_hz: u32) {
        self.tick_hz.store(tick_hz, Ordering::Release);
        #[cfg(feature = "rt")]
        {
            use hal_lxsis::ccompare::CcompareTimer;
            use hal::timer::{Timer, TimerMode};
            // ESP32-S3 runs at 240 MHz by default; adjust core_mhz as needed.
            let period_us = 1_000_000u64 / tick_hz as u64;
            let mut cmp = CcompareTimer::new(240);
            let _ = cmp.start(period_us, TimerMode::Periodic);
        }
    }

    /// Returns the currently configured tick frequency in hertz.
    pub fn tick_hz(&self) -> u32 {
        self.tick_hz.load(Ordering::Acquire)
    }
}
