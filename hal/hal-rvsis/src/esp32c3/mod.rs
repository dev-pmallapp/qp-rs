//! ESP32-C3 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;
pub mod intmtx;

pub use regs::GpioRegs;
pub use gpio::Esp32C3Pin;
pub use spi::Esp32C3Spi;
pub use uart::Esp32C3Uart;
pub use intmtx::Esp32C3IntMatrix;
