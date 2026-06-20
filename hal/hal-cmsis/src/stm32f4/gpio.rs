//! STM32F4 GPIO driver

use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;
use super::regs::GpioRegs;

/// STM32F4 GPIO pin implementation
pub struct Stm32F4Pin {
    regs: *const GpioRegs,
    pin: u8,
}

unsafe impl Send for Stm32F4Pin {}
unsafe impl Sync for Stm32F4Pin {}

impl Stm32F4Pin {
    /// Create a new Stm32F4Pin handle
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
impl GpioPin for Stm32F4Pin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let shift = (self.pin as u32) * 2;
        let moder = match mode {
            PinMode::Input | PinMode::InputPullUp | PinMode::InputPullDown => 0b00,
            PinMode::Output | PinMode::OutputOpenDrain                      => 0b01,
            PinMode::Alternate(_)                                           => 0b10,
        };
        self.regs().moder.modify(|v| (v & !(0b11 << shift)) | (moder << shift));
        
        let pupdr = match mode {
            PinMode::InputPullUp   => 0b01u32,
            PinMode::InputPullDown => 0b10,
            _                      => 0b00,
        };
        self.regs().pupdr.modify(|v| (v & !(0b11 << shift)) | (pupdr << shift));
        
        let otype = if mode == PinMode::OutputOpenDrain { 1u32 } else { 0 };
        self.regs().otyper.modify(|v| (v & !(1 << self.pin)) | (otype << self.pin));
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        Ok(if (self.regs().idr.read() >> self.pin) & 1 != 0 { Level::High } else { Level::Low })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        let mask = match level {
            Level::High => 1u32 << self.pin,
            Level::Low  => 1u32 << (self.pin + 16),
        };
        self.regs().bsrr.write(mask);
        Ok(())
    }

    fn pin_number(&self) -> u32 {
        self.pin as u32
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 digital pin impls
// ---------------------------------------------------------------------------
impl embedded_hal::digital::ErrorType for Stm32F4Pin {
    type Error = hal::error::HalError;
}

impl embedded_hal::digital::OutputPin for Stm32F4Pin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.regs().bsrr.write(1u32 << self.pin);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.regs().bsrr.write(1u32 << (self.pin + 16));
        Ok(())
    }
}

impl embedded_hal::digital::InputPin for Stm32F4Pin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().idr.read() >> self.pin) & 1 != 0)
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().idr.read() >> self.pin) & 1 == 0)
    }
}

impl embedded_hal::digital::StatefulOutputPin for Stm32F4Pin {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().odr.read() >> self.pin) & 1 != 0)
    }

    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok((self.regs().odr.read() >> self.pin) & 1 == 0)
    }
}
