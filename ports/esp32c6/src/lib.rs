//! QP Framework Port for ESP32-C6
//!
//! This is the **middleware layer** that integrates the QP framework with ESP32-C6 hardware.
//! It sits between the QP framework (qp-core, qp-qep, qp-qf, qp-qv) and the board support
//! package (boards/esp32c6).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────┐
//! │   Application (examples/dpp)    │
//! └────────────────┬────────────────┘
//!                  │
//! ┌────────────────▼────────────────┐
//! │  QP Framework (qp-core, etc.)   │
//! └────────────────┬────────────────┘
//!                  │
//! ┌────────────────▼────────────────┐
//! │  Port Layer (ports/esp32c6) ◄───┤ ← YOU ARE HERE
//! └────────────────┬────────────────┘
//!                  │
//! ┌────────────────▼────────────────┐
//! │  Board BSP (boards/esp32c6)     │
//! └────────────────┬────────────────┘
//!                  │
//! ┌────────────────▼────────────────┐
//! │  Hardware (ESP32-C6 chip)       │
//! └─────────────────────────────────┘
//! ```
//!
//! # Responsibilities
//!
//! - **Critical sections**: Implement QP critical section API using board primitives
//! - **Time management**: Setup tick timers and integrate with QP time events
//! - **Scheduler integration**: Connect QP scheduler with board event loop
//! - **Initialization**: Coordinate QP and board initialization sequence
//!
//! # Usage
//!
//! ```rust,no_run
//! #![no_std]
//! #![no_main]
//!
//! use qp_port_esp32c6 as qp_port;
//!
//! #[entry]
//! fn main() -> ! {
//!     // Initialize board (low-level hardware)
//!     let peripherals = qp_port::board::init();
//!     
//!     // Initialize QP port (middleware)
//!     qp_port::init();
//!     
//!     // Your QP application
//!     // ...
//!     
//!     qp_port::run()
//! }
//! ```

#![no_std]

pub mod critical;
pub mod scheduler;
pub mod time;

// Re-export board BSP for convenience
pub use qp_bsp_esp32c6 as board;

// Re-export QP framework types
pub use qp_core::{QEvent, QSignal, QStateHandler, QStateReturn, QStateMachine};
pub use qp_qep::QHsm;

/// Initialize the QP port middleware
///
/// This sets up the integration between QP framework and ESP32-C6 board.
/// Must be called after board initialization.
pub fn init() {
    time::init();
}

/// Run the QP application
///
/// Enters the main event loop and never returns.
pub fn run() -> ! {
    scheduler::run()
}

/// Stop the scheduler (for testing or cleanup)
pub fn stop() {
    scheduler::stop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        init();
    }
}
