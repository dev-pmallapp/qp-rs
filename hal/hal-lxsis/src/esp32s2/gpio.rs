//! ESP32-S2 GPIO driver

use hal::gpio::PinMode;
use hal::error::{HalError, HalResult};
use super::regs::gpio;

/// ESP32-S2 GPIO pin handle.
pub struct Esp32S2Pin {
    pin: u8,
}

unsafe impl Send for Esp32S2Pin {}
unsafe impl Sync for Esp32S2Pin {}

impl Esp32S2Pin {
    /// Create a new GPIO pin handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of `pin`.
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

impl embedded_hal::digital::ErrorType for Esp32S2Pin {
    type Error = HalError;
}

impl embedded_hal::digital::OutputPin for Esp32S2Pin {
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

impl embedded_hal::digital::InputPin for Esp32S2Pin {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(self.is_set_high())
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(!self.is_set_high())
    }
}
