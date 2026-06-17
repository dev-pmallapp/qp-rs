//! ESP32-S2 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;

pub use regs::GpioRegs;
pub use gpio::Esp32S2Pin;
pub use spi::Esp32S2Spi;
pub use uart::Esp32S2Uart;
