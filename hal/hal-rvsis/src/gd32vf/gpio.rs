//! GD32VF103 GPIO driver

use hal::gpio::{GpioPin, Level, PinMode};
use hal::error::HalResult;
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
}

impl GpioPin for Gd32VfPin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
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

    fn read(&self) -> HalResult<Level> {
        let val = (self.regs().istat.read() >> self.pin) & 1;
        Ok(if val != 0 { Level::High } else { Level::Low })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        match level {
            Level::High => self.regs().bop.write(1 << self.pin),
            Level::Low => self.regs().bop.write(1 << (self.pin + 16)),
        }
        Ok(())
    }

    fn pin_number(&self) -> u32 {
        self.pin as u32
    }
}
