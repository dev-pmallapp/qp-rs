//! LPC1768 Fast GPIO driver

use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;
use super::regs::gpio_port;

/// LPC1768 GPIO pin handle.
///
/// Identifies a pin by port (0–4) and bit position within the port (0–31).
pub struct Lpc17Pin {
    port: u8,
    pin:  u8,
}

unsafe impl Send for Lpc17Pin {}
unsafe impl Sync for Lpc17Pin {}

impl Lpc17Pin {
    /// Create a new GPIO pin handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of `port`/`pin`.
    pub unsafe fn new(port: u8, pin: u8) -> Self {
        Self { port, pin }
    }
}

#[allow(deprecated)]
impl GpioPin for Lpc17Pin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let mask = 1u32 << self.pin;
        let regs = unsafe { gpio_port(self.port) };
        match mode {
            PinMode::Output | PinMode::OutputOpenDrain => {
                regs.dir.modify(|v| v | mask);
            }
            _ => {
                regs.dir.modify(|v| v & !mask);
            }
        }
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        let regs = unsafe { gpio_port(self.port) };
        Ok(if (regs.pin.read() & (1 << self.pin)) != 0 {
            Level::High
        } else {
            Level::Low
        })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        let regs = unsafe { gpio_port(self.port) };
        let mask = 1u32 << self.pin;
        match level {
            Level::High => regs.set.write(mask),
            Level::Low  => regs.clr.write(mask),
        }
        Ok(())
    }

    fn pin_number(&self) -> u32 {
        self.port as u32 * 32 + self.pin as u32
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 digital pin impls
// ---------------------------------------------------------------------------
impl embedded_hal::digital::ErrorType for Lpc17Pin {
    type Error = hal::error::HalError;
}

impl embedded_hal::digital::OutputPin for Lpc17Pin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        regs.set.write(1u32 << self.pin);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        regs.clr.write(1u32 << self.pin);
        Ok(())
    }
}

impl embedded_hal::digital::InputPin for Lpc17Pin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        Ok((regs.pin.read() & (1 << self.pin)) != 0)
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        Ok((regs.pin.read() & (1 << self.pin)) == 0)
    }
}

impl embedded_hal::digital::StatefulOutputPin for Lpc17Pin {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        Ok((regs.pin.read() & (1 << self.pin)) != 0)
    }

    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        let regs = unsafe { gpio_port(self.port) };
        Ok((regs.pin.read() & (1 << self.pin)) == 0)
    }
}
