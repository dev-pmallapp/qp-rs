//! STM32F4 vendor module

pub mod regs;
pub mod gpio;
pub mod spi;
pub mod uart;
pub mod i2c;

pub use regs::GpioRegs;
pub use gpio::Stm32F4Pin;
pub use spi::Stm32F4Spi;
pub use uart::Stm32F4Uart;
pub use i2c::Stm32F4I2c;
