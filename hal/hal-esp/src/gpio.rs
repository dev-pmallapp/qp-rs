//! ESP32 GPIO implementation using ESP-IDF

use hal::gpio::{Edge, GpioPin, GpioPinInterrupt, Level, PinMode};
use hal::error::{HalError, HalResult};
use esp_idf_sys as sys;
use core::sync::atomic::{AtomicBool, Ordering};

/// ESP32 GPIO pin implementation
pub struct EspGpioPin {
    pin: u32,
    mode: PinMode,
}

impl EspGpioPin {
    /// Create a new GPIO pin
    ///
    /// # Arguments
    /// * `pin` - GPIO pin number (0-47 depending on chip)
    pub fn new(pin: u32) -> HalResult<Self> {
        // Validate pin number (ESP32-S3 has 0-48, but some are restricted)
        if pin >= 49 {
            return Err(HalError::InvalidParameter);
        }

        Ok(Self {
            pin,
            mode: PinMode::Input,
        })
    }

    /// Get the pin number
    pub fn pin_number(&self) -> u32 {
        self.pin
    }
}

impl GpioPin for EspGpioPin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let esp_mode = match mode {
            PinMode::Input => sys::gpio_mode_t_GPIO_MODE_INPUT,
            PinMode::InputPullUp => sys::gpio_mode_t_GPIO_MODE_INPUT,
            PinMode::InputPullDown => sys::gpio_mode_t_GPIO_MODE_INPUT,
            PinMode::Output => sys::gpio_mode_t_GPIO_MODE_OUTPUT,
            PinMode::OutputOpenDrain => sys::gpio_mode_t_GPIO_MODE_OUTPUT_OD,
            PinMode::Alternate(_) => return Err(HalError::NotSupported),
        };

        unsafe {
            // Set GPIO direction
            let ret = sys::gpio_set_direction(self.pin as i32, esp_mode);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Configure pull-up/down if needed
            match mode {
                PinMode::InputPullUp => {
                    let ret = sys::gpio_set_pull_mode(
                        self.pin as i32,
                        sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY,
                    );
                    if ret != sys::ESP_OK as i32 {
                        return Err(HalError::VendorError(ret));
                    }
                }
                PinMode::InputPullDown => {
                    let ret = sys::gpio_set_pull_mode(
                        self.pin as i32,
                        sys::gpio_pull_mode_t_GPIO_PULLDOWN_ONLY,
                    );
                    if ret != sys::ESP_OK as i32 {
                        return Err(HalError::VendorError(ret));
                    }
                }
                _ => {
                    // Disable pull-up/down for other modes
                    let ret = sys::gpio_set_pull_mode(
                        self.pin as i32,
                        sys::gpio_pull_mode_t_GPIO_FLOATING,
                    );
                    if ret != sys::ESP_OK as i32 {
                        return Err(HalError::VendorError(ret));
                    }
                }
            }
        }

        self.mode = mode;
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        unsafe {
            let level = sys::gpio_get_level(self.pin as i32);
            Ok(if level != 0 { Level::High } else { Level::Low })
        }
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        unsafe {
            let val = match level {
                Level::Low => 0,
                Level::High => 1,
            };
            let ret = sys::gpio_set_level(self.pin as i32, val);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }
        Ok(())
    }

    fn toggle(&mut self) -> HalResult<()> {
        let current = self.read()?;
        let new_level = match current {
            Level::Low => Level::High,
            Level::High => Level::Low,
        };
        self.write(new_level)
    }

    fn pin_number(&self) -> u32 {
        self.pin
    }
}

impl GpioPinInterrupt for EspGpioPin {
    fn enable_interrupt(&mut self, edge: Edge) -> HalResult<()> {
        let intr_type = match edge {
            Edge::Rising => sys::gpio_int_type_t_GPIO_INTR_POSEDGE,
            Edge::Falling => sys::gpio_int_type_t_GPIO_INTR_NEGEDGE,
            Edge::Both => sys::gpio_int_type_t_GPIO_INTR_ANYEDGE,
        };

        unsafe {
            // Set interrupt type
            let ret = sys::gpio_set_intr_type(self.pin as i32, intr_type);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Install ISR service if not already installed
            static ISR_SERVICE_INSTALLED: AtomicBool = AtomicBool::new(false);
            if !ISR_SERVICE_INSTALLED.load(Ordering::Acquire) {
                let ret = sys::gpio_install_isr_service(0);
                // ESP_ERR_INVALID_STATE means already installed, which is fine
                if ret != sys::ESP_OK as i32 && ret != sys::ESP_ERR_INVALID_STATE as i32 {
                    return Err(HalError::VendorError(ret));
                }
                ISR_SERVICE_INSTALLED.store(true, Ordering::Release);
            }

            // Enable interrupt
            let ret = sys::gpio_intr_enable(self.pin as i32);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        Ok(())
    }

    fn disable_interrupt(&mut self) -> HalResult<()> {
        unsafe {
            let ret = sys::gpio_intr_disable(self.pin as i32);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }
        Ok(())
    }

    fn clear_interrupt(&mut self) -> HalResult<()> {
        // ESP-IDF automatically clears interrupts when acknowledged
        // No explicit clear needed for GPIO interrupts
        Ok(())
    }

    fn is_interrupt_pending(&self) -> bool {
        // ESP-IDF doesn't provide a direct way to query interrupt pending state
        // This would need to be tracked externally in the ISR handler
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_creation() {
        let pin = EspGpioPin::new(2);
        assert!(pin.is_ok());
        assert_eq!(pin.unwrap().pin_number(), 2);
    }

    #[test]
    fn test_invalid_pin() {
        let pin = EspGpioPin::new(100);
        assert!(pin.is_err());
    }
}
