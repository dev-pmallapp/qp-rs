//! ESP32-C6 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;
pub mod intmtx;

pub use regs::GpioRegs;
pub use gpio::Esp32C6Pin;
pub use spi::Esp32C6Spi;
pub use uart::Esp32C6Uart;
pub use intmtx::Esp32C6IntMatrix;
