//! SPI master implementation via ESP-IDF `spi_device_*` API.

use hal::error::{HalError, HalResult};
use hal::spi::{BitOrder, SpiConfig, SpiMaster, SpiMode};

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
use esp_idf_sys as sys;

pub struct EspSpiMaster {
    #[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
    host:   sys::spi_host_device_t,
    #[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
    handle: sys::spi_device_handle_t,
    cs_pin: i32,
    config: SpiConfig,
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl EspSpiMaster {
    pub fn new(host: u8, cs_pin: i32, config: &SpiConfig) -> HalResult<Self> {
        let spi_host = match host {
            2 => sys::spi_host_device_t_SPI2_HOST,
            3 => sys::spi_host_device_t_SPI3_HOST,
            _ => return Err(HalError::InvalidParameter),
        };
        let mode = match config.mode {
            SpiMode::Mode0 => 0u8, SpiMode::Mode1 => 1,
            SpiMode::Mode2 => 2,   SpiMode::Mode3 => 3,
        };
        let lsb = config.bit_order == BitOrder::LsbFirst;
        let dev_cfg = sys::spi_device_interface_config_t {
            mode,
            clock_speed_hz: config.frequency as i32,
            spics_io_num: cs_pin,
            flags: if lsb { sys::SPI_DEVICE_BIT_LSBFIRST } else { 0 },
            queue_size: 1,
            ..Default::default()
        };
        let mut handle: sys::spi_device_handle_t = core::ptr::null_mut();
        let ret = unsafe { sys::spi_bus_add_device(spi_host, &dev_cfg, &mut handle) };
        if ret != sys::ESP_OK as i32 { return Err(HalError::VendorError(ret)); }
        Ok(Self { host: spi_host, handle, cs_pin, config: config.clone() })
    }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl SpiMaster for EspSpiMaster {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        // Changing SPI device parameters requires removing and re-adding the device.
        let ret = unsafe { sys::spi_bus_remove_device(self.handle) };
        if ret != sys::ESP_OK as i32 {
            return Err(HalError::VendorError(ret));
        }
        let mode = match config.mode {
            SpiMode::Mode0 => 0u8, SpiMode::Mode1 => 1,
            SpiMode::Mode2 => 2,   SpiMode::Mode3 => 3,
        };
        let lsb = config.bit_order == BitOrder::LsbFirst;
        let dev_cfg = sys::spi_device_interface_config_t {
            mode,
            clock_speed_hz: config.frequency as i32,
            spics_io_num: self.cs_pin,
            flags: if lsb { sys::SPI_DEVICE_BIT_LSBFIRST } else { 0 },
            queue_size: 1,
            ..Default::default()
        };
        let ret = unsafe { sys::spi_bus_add_device(self.host, &dev_cfg, &mut self.handle) };
        if ret != sys::ESP_OK as i32 {
            return Err(HalError::VendorError(ret));
        }
        self.config = config.clone();
        Ok(())
    }

    fn transfer(&mut self, tx: &[u8], rx: &mut [u8]) -> HalResult<()> {
        if tx.len() != rx.len() { return Err(HalError::InvalidParameter); }
        let mut t = sys::spi_transaction_t {
            length: (tx.len() * 8) as usize,
            rxlength: (rx.len() * 8) as usize,
            __bindgen_anon_3: sys::spi_transaction_t__bindgen_ty_3 {
                tx_buffer: tx.as_ptr() as *const _,
            },
            __bindgen_anon_4: sys::spi_transaction_t__bindgen_ty_4 {
                rx_buffer: rx.as_mut_ptr() as *mut _,
            },
            ..Default::default()
        };
        let ret = unsafe { sys::spi_device_polling_transmit(self.handle, &mut t) };
        if ret != sys::ESP_OK as i32 { Err(HalError::VendorError(ret)) } else { Ok(()) }
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        let mut dummy = [0u8; 1];
        for b in data {
            self.transfer(core::slice::from_ref(b), &mut dummy)?;
        }
        Ok(())
    }

    fn read(&mut self, buf: &mut [u8]) -> HalResult<()> {
        let zeros = [0u8; 1];
        for slot in buf.iter_mut() {
            let mut r = [0u8; 1];
            self.transfer(&zeros, &mut r)?;
            *slot = r[0];
        }
        Ok(())
    }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl Drop for EspSpiMaster {
    fn drop(&mut self) {
        unsafe { sys::spi_bus_remove_device(self.handle); }
    }
}
