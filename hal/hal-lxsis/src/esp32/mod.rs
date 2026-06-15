//! ESP32 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;

pub use regs::GpioRegs;
pub use gpio::Esp32Pin;
pub use spi::Esp32Spi;
pub use uart::Esp32Uart;
