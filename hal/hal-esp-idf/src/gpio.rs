//! GPIO implementation via ESP-IDF `gpio_*` API.
//!
//! Wraps `esp_idf_sys::gpio_set_direction`, `gpio_set_level`, etc.
//! For ISR-safe GPIO use `hal-lxsis`/`hal-rvsis` direct register access instead.

use hal::error::{HalError, HalResult};
use hal::gpio::{Edge, GpioPin, GpioPinInterrupt, Level, PinMode};

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
use esp_idf_sys as sys;

pub struct EspGpioPin {
    pin:  u32,
    mode: PinMode,
}

impl EspGpioPin {
    pub fn new(pin: u32) -> HalResult<Self> {
        if pin >= 49 { return Err(HalError::InvalidParameter); }
        Ok(Self { pin, mode: PinMode::Input })
    }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl GpioPin for EspGpioPin {
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()> {
        let esp_mode = match mode {
            PinMode::Input | PinMode::InputPullUp | PinMode::InputPullDown =>
                sys::gpio_mode_t_GPIO_MODE_INPUT,
            PinMode::Output =>
                sys::gpio_mode_t_GPIO_MODE_OUTPUT,
            PinMode::OutputOpenDrain =>
                sys::gpio_mode_t_GPIO_MODE_OUTPUT_OD,
            PinMode::Alternate(_) => return Err(HalError::NotSupported),
        };
        let ret = unsafe { sys::gpio_set_direction(self.pin as i32, esp_mode) };
        if ret != sys::ESP_OK as i32 { return Err(HalError::VendorError(ret)); }
        let pull = match mode {
            PinMode::InputPullUp   => sys::gpio_pull_mode_t_GPIO_PULLUP_ONLY,
            PinMode::InputPullDown => sys::gpio_pull_mode_t_GPIO_PULLDOWN_ONLY,
            _                      => sys::gpio_pull_mode_t_GPIO_FLOATING,
        };
        let ret = unsafe { sys::gpio_set_pull_mode(self.pin as i32, pull) };
        if ret != sys::ESP_OK as i32 { return Err(HalError::VendorError(ret)); }
        self.mode = mode;
        Ok(())
    }

    fn read(&self) -> HalResult<Level> {
        let v = unsafe { sys::gpio_get_level(self.pin as i32) };
        Ok(if v != 0 { Level::High } else { Level::Low })
    }

    fn write(&mut self, level: Level) -> HalResult<()> {
        let v = match level { Level::High => 1, Level::Low => 0 };
        let ret = unsafe { sys::gpio_set_level(self.pin as i32, v) };
        if ret != sys::ESP_OK as i32 { Err(HalError::VendorError(ret)) } else { Ok(()) }
    }

    fn pin_number(&self) -> u32 { self.pin }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl GpioPinInterrupt for EspGpioPin {
    fn enable_interrupt(&mut self, edge: Edge) -> HalResult<()> {
        let intr = match edge {
            Edge::Rising  => sys::gpio_int_type_t_GPIO_INTR_POSEDGE,
            Edge::Falling => sys::gpio_int_type_t_GPIO_INTR_NEGEDGE,
            Edge::Both    => sys::gpio_int_type_t_GPIO_INTR_ANYEDGE,
        };
        let ret = unsafe { sys::gpio_set_intr_type(self.pin as i32, intr) };
        if ret != sys::ESP_OK as i32 { return Err(HalError::VendorError(ret)); }
        let ret = unsafe { sys::gpio_intr_enable(self.pin as i32) };
        if ret != sys::ESP_OK as i32 { Err(HalError::VendorError(ret)) } else { Ok(()) }
    }

    fn disable_interrupt(&mut self) -> HalResult<()> {
        let ret = unsafe { sys::gpio_intr_disable(self.pin as i32) };
        if ret != sys::ESP_OK as i32 { Err(HalError::VendorError(ret)) } else { Ok(()) }
    }

    fn clear_interrupt(&mut self) -> HalResult<()> { Ok(()) }
    fn is_interrupt_pending(&self) -> bool { false }
}
