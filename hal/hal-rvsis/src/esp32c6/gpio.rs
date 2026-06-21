//! ESP32-C6 GPIO driver

use hal::gpio::PinMode;
use hal::error::{HalError, HalResult};
use super::regs::gpio;

/// ESP32-C6 GPIO Pin
pub struct Esp32C6Pin {
    pin: u8,
}

unsafe impl Send for Esp32C6Pin {}
unsafe impl Sync for Esp32C6Pin {}

impl Esp32C6Pin {
    /// Create a new Esp32C6Pin handle
    ///
    /// # Safety
    /// Unique ownership of the GPIO pin must be guaranteed by the caller.
    pub unsafe fn new(pin: u8) -> Self {
        Self { pin }
    }

    /// Configure the pin direction. Call before driving/reading the pin, since
    /// embedded-hal pins are assumed pre-configured.
    pub fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let pin = self.pin;
        let is_output = matches!(mode, PinMode::Output | PinMode::OutputOpenDrain);

        if is_output {
            if pin < 32 {
                gpio().enable_w1ts.write(1 << pin);
            } else {
                gpio().enable1_w1ts.write(1 << (pin - 32));
            }
        } else if pin < 32 {
            gpio().enable_w1tc.write(1 << pin);
        } else {
            gpio().enable1_w1tc.write(1 << (pin - 32));
        }
        Ok(())
    }

    /// Pin number on the port.
    pub fn pin_number(&self) -> u32 {
        self.pin as u32
    }

    fn is_set_high(&self) -> bool {
        let pin = self.pin;
        let val = if pin < 32 {
            (gpio().in_.read() >> pin) & 1
        } else {
            (gpio().in1.read() >> (pin - 32)) & 1
        };
        val != 0
    }
}

impl embedded_hal::digital::ErrorType for Esp32C6Pin {
    type Error = HalError;
}

impl embedded_hal::digital::OutputPin for Esp32C6Pin {
    fn set_high(&mut self) -> Result<(), Self::Error> {
        let pin = self.pin;
        if pin < 32 {
            gpio().out_w1ts.write(1 << pin);
        } else {
            gpio().out1_w1ts.write(1 << (pin - 32));
        }
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), Self::Error> {
        let pin = self.pin;
        if pin < 32 {
            gpio().out_w1tc.write(1 << pin);
        } else {
            gpio().out1_w1tc.write(1 << (pin - 32));
        }
        Ok(())
    }
}

impl embedded_hal::digital::InputPin for Esp32C6Pin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.is_set_high())
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.is_set_high())
    }
}
