//! GD32VF103 GPIO driver

use hal::gpio::PinMode;
use hal::error::{HalError, HalResult};
use super::regs::GpioRegs;

/// GD32VF103 GPIO Pin
pub struct Gd32VfPin {
    regs: *const GpioRegs,
    pin: u8,
}

unsafe impl Send for Gd32VfPin {}
unsafe impl Sync for Gd32VfPin {}

impl Gd32VfPin {
    /// Create a new Gd32VfPin handle
    ///
    /// # Safety
    /// Unique ownership of the GPIO port and pin must be guaranteed by the caller.
    pub unsafe fn new(regs: *const GpioRegs, pin: u8) -> Self {
        Self { regs, pin }
    }

    fn regs(&self) -> &GpioRegs {
        unsafe { &*self.regs }
    }

    /// Configure the pin direction. Call before driving/reading the pin, since
    /// embedded-hal pins are assumed pre-configured.
    pub fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let (md, ctl) = match mode {
            PinMode::Input => (0b00, 0b01),      // Input, floating
            PinMode::InputPullUp => {
                // Input with pull-up/pull-down (ostat determines pull-up)
                self.regs().ostat.modify(|v| v | (1 << self.pin));
                (0b00, 0b10)
            }
            PinMode::InputPullDown => {
                // Input with pull-up/pull-down (ostat determines pull-down)
                self.regs().ostat.modify(|v| v & !(1 << self.pin));
                (0b00, 0b10)
            }
            PinMode::Output => (0b11, 0b00),     // Output 50MHz, general purpose push-pull
            PinMode::OutputOpenDrain => (0b11, 0b01), // Output 50MHz, general purpose open-drain
            PinMode::Alternate(_) => (0b11, 0b10), // Output 50MHz, alternate function push-pull
        };

        let val = (ctl << 2) | md;
        if self.pin < 8 {
            let shift = (self.pin as u32) * 4;
            self.regs().ctl0.modify(|v| (v & !(0b1111 << shift)) | (val << shift));
        } else {
            let shift = ((self.pin - 8) as u32) * 4;
            self.regs().ctl1.modify(|v| (v & !(0b1111 << shift)) | (val << shift));
        }

        Ok(())
    }

    /// Pin number on the port.
    pub fn pin_number(&self) -> u32 {
        self.pin as u32
    }

    fn is_set_high(&self) -> bool {
        ((self.regs().istat.read() >> self.pin) & 1) != 0
    }
}

impl embedded_hal::digital::ErrorType for Gd32VfPin {
    type Error = HalError;
}

impl embedded_hal::digital::OutputPin for Gd32VfPin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.regs().bop.write(1 << self.pin);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.regs().bop.write(1 << (self.pin + 16));
        Ok(())
    }
}

impl embedded_hal::digital::InputPin for Gd32VfPin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.is_set_high())
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.is_set_high())
    }
}
