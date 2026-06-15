//! ESP32-S3 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;

pub use regs::GpioRegs;
pub use gpio::Esp32S3Pin;
pub use spi::Esp32S3Spi;
pub use uart::Esp32S3Uart;
