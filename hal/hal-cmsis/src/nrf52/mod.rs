//! nRF52840 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;
pub mod i2c;

pub use regs::GpioRegs;
pub use gpio::Nrf52Pin;
pub use spi::Nrf52Spi;
pub use uart::Nrf52Uart;
pub use i2c::Nrf52I2c;
