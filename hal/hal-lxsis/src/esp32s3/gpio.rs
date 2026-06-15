//! ESP32-S3 GPIO driver

use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;
use super::regs::gpio;

/// ESP32-S3 GPIO Pin
pub struct Esp32S3Pin {
    pin: u8,
}

unsafe impl Send for Esp32S3Pin {}
unsafe impl Sync for Esp32S3Pin {}

impl Esp32S3Pin {
    /// Create a new Esp32S3Pin handle
    ///
    /// # Safety
    /// Unique ownership of the GPIO pin must be guaranteed by the caller.
    pub unsafe fn new(pin: u8) -> Self {
        Self { pin }
    }
}

impl GpioPin for Esp32S3Pin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let pin = self.pin;
        let is_output = match mode {
            PinMode::Output | PinMode::OutputOpenDrain => true,
            _ => false,
        };

        if is_output {
            if pin < 32 {
                gpio().enable_w1ts.write(1 << pin);
            } else {
                gpio().enable1_w1ts.write(1 << (pin - 32));
            }
        } else {
            if pin < 32 {
                gpio().enable_w1tc.write(1 << pin);
            } else {
                gpio().enable1_w1tc.write(1 << (pin - 32));
            }
        }
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        let pin = self.pin;
        let val = if pin < 32 {
            (gpio().in_.read() >> pin) & 1
        } else {
            (gpio().in1.read() >> (pin - 32)) & 1
        };

        Ok(if val != 0 { Level::High } else { Level::Low })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        let pin = self.pin;
        match level {
            Level::High => {
                if pin < 32 {
                    gpio().out_w1ts.write(1 << pin);
                } else {
                    gpio().out1_w1ts.write(1 << (pin - 32));
                }
            }
            Level::Low => {
                if pin < 32 {
                    gpio().out_w1tc.write(1 << pin);
                } else {
                    gpio().out1_w1tc.write(1 << (pin - 32));
                }
            }
        }
        Ok(())
    }

    fn pin_number(&self) -> u32 {
        self.pin as u32
    }
}
