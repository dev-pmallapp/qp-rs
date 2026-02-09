#![no_std]

//! ESP32-S3 port scaffolding for the Quantum Platform kernels.
//!
//! This crate currently provides architectural placeholders for the interrupt
//! controller and system timer integration required by [`qk`] on Espressif's
//! ESP32-S3 SoC. The implementation will evolve as hardware-specific HAL
//! bindings are introduced.

#[cfg(feature = "rt")]
extern crate alloc;

pub mod interrupts;
pub mod timer;

#[cfg(feature = "rt")]
pub mod runtime;

pub use interrupts::{InterruptController, SchedulerGuard};
pub use timer::SystemTimer;

#[cfg(feature = "rt")]
pub use runtime::Esp32S3QkRuntime;

/// Aggregates subsystems managed by the ESP32-S3 port.
#[derive(Debug)]
pub struct Esp32S3Port {
    interrupts: InterruptController,
    timer: SystemTimer,
}

impl Esp32S3Port {
    /// Creates a new port instance with default interrupt and timer handlers.
    pub const fn new() -> Self {
        Self {
            interrupts: InterruptController::new(),
            timer: SystemTimer::new(),
        }
    }

    /// Returns the interrupt controller responsible for scheduler locking.
    pub const fn interrupts(&self) -> &InterruptController {
        &self.interrupts
    }

    /// Returns the mutable interrupt controller.
    pub fn interrupts_mut(&mut self) -> &mut InterruptController {
        &mut self.interrupts
    }

    /// Returns the system timer abstraction used to drive QK ticks.
    pub const fn timer(&self) -> &SystemTimer {
        &self.timer
    }

    /// Returns the mutable system timer.
    pub fn timer_mut(&mut self) -> &mut SystemTimer {
        &mut self.timer
    }

    /// Placeholder for interrupt routing initialisation.
    pub fn init_interrupts(&mut self) {
        self.interrupts.configure_priorities();
    }

    /// Placeholder for configuring the hardware timer that drives `TimeEvent`s.
    pub fn init_system_timer(&mut self, tick_hz: u32) {
        self.timer.configure_periodic(tick_hz);
    }
}

impl Default for Esp32S3Port {
    fn default() -> Self {
        Self::new()
    }
}

/// Optional port configuration shared with future BSP glue.
#[derive(Debug, Clone, Copy)]
pub struct PortConfig {
    pub enable_trace: bool,
    pub tick_hz: u32,
}

impl PortConfig {
    pub const fn new() -> Self {
        Self {
            enable_trace: true,
            tick_hz: 1000,
        }
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_initialises_subsystems() {
        let mut port = Esp32S3Port::new();
        assert_eq!(port.timer().tick_hz(), 0);

        port.init_system_timer(1000);
        assert_eq!(port.timer().tick_hz(), 1000);
    }
}
