#![no_std]

//! ESP32-C6 port scaffolding for the Quantum Platform kernels.
//!
//! This crate currently mirrors the ESP32-S3 port but targets Espressif's
//! RISC-V based ESP32-C6. All interrupt controller and timer plumbing remain
//! placeholders until real HAL bindings are introduced.

#[cfg(feature = "rt")]
extern crate alloc;

pub mod interrupts;
pub mod timer;

#[cfg(feature = "rt")]
pub mod runtime;

pub use interrupts::{InterruptController, SchedulerGuard};
pub use timer::SystemTimer;

#[cfg(feature = "rt")]
pub use runtime::Esp32C6QkRuntime;

/// Aggregates the platform-specific subsystems managed by the ESP32-C6 port.
#[derive(Debug)]
pub struct Esp32C6Port {
    interrupts: InterruptController,
    timer: SystemTimer,
}

impl Esp32C6Port {
    /// Creates a new port instance using default interrupt and timer handlers.
    pub const fn new() -> Self {
        Self {
            interrupts: InterruptController::new(),
            timer: SystemTimer::new(),
        }
    }

    /// Returns the interrupt controller that mediates scheduler locking.
    pub const fn interrupts(&self) -> &InterruptController {
        &self.interrupts
    }

    /// Returns the mutable interrupt controller.
    pub fn interrupts_mut(&mut self) -> &mut InterruptController {
        &mut self.interrupts
    }

    /// Returns a reference to the system timer abstraction.
    pub const fn timer(&self) -> &SystemTimer {
        &self.timer
    }

    /// Returns a mutable reference to the system timer.
    pub fn timer_mut(&mut self) -> &mut SystemTimer {
        &mut self.timer
    }

    /// Placeholder for configuring interrupt priorities.
    pub fn init_interrupts(&mut self) {
        self.interrupts.configure_priorities();
    }

    /// Placeholder for configuring the hardware timer that drives `TimeEvent`s.
    pub fn init_system_timer(&mut self, tick_hz: u32) {
        self.timer.configure_periodic(tick_hz);
    }
}

impl Default for Esp32C6Port {
    fn default() -> Self {
        Self::new()
    }
}

/// Optional runtime configuration shared with board support glue.
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
        let mut port = Esp32C6Port::new();
        assert_eq!(port.timer().tick_hz(), 0);

        port.init_system_timer(1000);
        assert_eq!(port.timer().tick_hz(), 1000);
    }
}
