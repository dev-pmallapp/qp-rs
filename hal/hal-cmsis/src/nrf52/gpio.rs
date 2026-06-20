//! nRF52840 GPIO driver

use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;
use super::regs::GpioRegs;

/// nRF52 GPIO pin implementation
pub struct Nrf52Pin {
    regs: *const GpioRegs,
    pin: u8,
}

unsafe impl Send for Nrf52Pin {}
unsafe impl Sync for Nrf52Pin {}

impl Nrf52Pin {
    /// Create a new Nrf52Pin handle
    ///
    /// # Safety
    /// Unique ownership of this GPIO port and pin must be guaranteed by the caller.
    pub unsafe fn new(regs: *const GpioRegs, pin: u8) -> Self {
        Self { regs, pin }
    }

    fn regs(&self) -> &GpioRegs {
        unsafe { &*self.regs }
    }
}

#[allow(deprecated)]
impl GpioPin for Nrf52Pin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let pin_cnf = match mode {
            PinMode::Input => 0, // Input buffer connected, pull disabled, drive S0S1, sense disabled
            PinMode::InputPullUp => (1 << 2) | (3 << 4), // Input pullup
            PinMode::InputPullDown => (1 << 2) | (1 << 4), // Input pulldown
            PinMode::Output => 1 | (3 << 8), // Output, input buffer disconnected
            PinMode::OutputOpenDrain => 1 | (6 << 8), // Output, drive S0D1 (Open drain)
            PinMode::Alternate(_) => {
                // nRF52 alternate functions are selected via peripheral PSEL registers.
                // We just configure the pin as connected to allow input/output.
                0
            }
        };
        self.regs().pin_cnf[self.pin as usize].write(pin_cnf);
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        let val = self.regs().in_.read();
        Ok(if (val & (1 << self.pin)) != 0 {
            Level::High
        } else {
            Level::Low
        })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        match level {
            Level::High => self.regs().outset.write(1 << self.pin),
            Level::Low  => self.regs().outclr.write(1 << self.pin),
        }
        Ok(())
    }

    fn pin_number(&self) -> u32 {
        self.pin as u32
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 digital pin impls
// ---------------------------------------------------------------------------
impl embedded_hal::digital::ErrorType for Nrf52Pin {
    type Error = hal::error::HalError;
}

impl embedded_hal::digital::OutputPin for Nrf52Pin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.regs().outset.write(1u32 << self.pin);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.regs().outclr.write(1u32 << self.pin);
        Ok(())
    }
}

impl embedded_hal::digital::InputPin for Nrf52Pin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().in_.read() & (1 << self.pin)) != 0)
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().in_.read() & (1 << self.pin)) == 0)
    }
}

impl embedded_hal::digital::StatefulOutputPin for Nrf52Pin {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().out.read() & (1 << self.pin)) != 0)
    }

    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().out.read() & (1 << self.pin)) == 0)
    }
}
