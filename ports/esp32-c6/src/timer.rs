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
        // HAL TODO: Replace with ESP-IDF GPTimer setup for RISC-V target.
        self.tick_hz.store(tick_hz, Ordering::Release);
    }

    /// Returns the currently configured tick frequency in hertz.
    pub fn tick_hz(&self) -> u32 {
        self.tick_hz.load(Ordering::Acquire)
    }
}
