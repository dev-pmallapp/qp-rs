use core::sync::atomic::{AtomicU32, Ordering};

/// Placeholder for the ESP32-S3 system timer driving QK ticks.
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
    ///
    /// # HAL Integration Plan
    ///
    /// When porting to real hardware, this method will:
    /// - Select a GPTimer instance using ESP-IDF HAL APIs
    /// - Configure the timer for periodic interrupts at `tick_hz`
    /// - Register an ISR that calls into the QK kernel's `tick()`
    /// - Ensure safe sharing of timer resources with other subsystems
    ///
    /// For now, only the tick frequency is tracked.
    pub fn configure_periodic(&self, tick_hz: u32) {
        // HAL TODO: Replace with ESP-IDF GPTimer setup
        // Example (pseudo-code):
        // let gptimer = esp_idf_hal::gptimer::Gptimer::new(...);
        // gptimer.set_periodic(tick_hz);
        // gptimer.register_isr(|| QkKernel::tick());
        self.tick_hz.store(tick_hz, Ordering::Release);
    }

    /// Returns the currently configured tick frequency in hertz.
    pub fn tick_hz(&self) -> u32 {
        self.tick_hz.load(Ordering::Acquire)
    }
}
