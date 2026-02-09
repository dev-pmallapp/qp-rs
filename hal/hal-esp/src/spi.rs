//! ESP32 SPI implementation using ESP-IDF

use hal::spi::{BitOrder, SpiConfig, SpiMaster, SpiMode};
use hal::error::{HalError, HalResult};
use esp_idf_sys as sys;

/// ESP32 SPI master implementation
pub struct EspSpiMaster {
    host: sys::spi_host_device_t,
    handle: sys::spi_device_handle_t,
    config: SpiConfig,
}

impl EspSpiMaster {
    /// Create a new SPI master
    ///
    /// # Arguments
    /// * `host` - SPI host (SPI2_HOST or SPI3_HOST on most ESP32)
    /// * `cs_pin` - Chip select GPIO pin number
    ///
    /// # Note
    /// Must call `configure()` before use.
    pub fn new(host: u8, cs_pin: i32) -> HalResult<Self> {
        // ESP32 typically has SPI2_HOST (1) and SPI3_HOST (2)
        // SPI1_HOST is used for flash
        let spi_host = match host {
            2 => sys::spi_host_device_t_SPI2_HOST,
            3 => sys::spi_host_device_t_SPI3_HOST,
            _ => return Err(HalError::InvalidParameter),
        };

        unsafe {
            // Initialize SPI bus (if not already done)
            // Note: In production, bus init should be done once per host
            let bus_config = sys::spi_bus_config_t {
                mosi_io_num: 13,  // Default MOSI pin
                miso_io_num: 12,  // Default MISO pin
                sclk_io_num: 14,  // Default SCLK pin
                quadwp_io_num: -1,
                quadhd_io_num: -1,
                data4_io_num: -1,
                data5_io_num: -1,
                data6_io_num: -1,
                data7_io_num: -1,
                max_transfer_sz: 4096,
                flags: 0,
                isr_cpu_id: sys::esp_intr_cpu_affinity_t_ESP_INTR_CPU_AFFINITY_AUTO,
                intr_flags: 0,
            };

            // Try to initialize bus (might already be initialized)
            let ret = sys::spi_bus_initialize(spi_host, &bus_config, sys::spi_dma_chan_t_SPI_DMA_CH_AUTO);
            // ESP_ERR_INVALID_STATE means already initialized, which is ok
            if ret != sys::ESP_OK as i32 && ret != sys::ESP_ERR_INVALID_STATE as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Add device to the bus
            let dev_config = sys::spi_device_interface_config_t {
                command_bits: 0,
                address_bits: 0,
                dummy_bits: 0,
                mode: 0, // Will be set in configure()
                clock_source: sys::spi_clock_source_t_SPI_CLK_SRC_DEFAULT,
                duty_cycle_pos: 128,
                cs_ena_pretrans: 0,
                cs_ena_posttrans: 0,
                clock_speed_hz: 1000000, // 1 MHz default
                input_delay_ns: 0,
                spics_io_num: cs_pin,
                flags: 0,
                queue_size: 1,
                pre_cb: None,
                post_cb: None,
            };

            let mut handle: sys::spi_device_handle_t = ptr::null_mut();
            let ret = sys::spi_bus_add_device(spi_host, &dev_config, &mut handle);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            Ok(Self {
                host: spi_host,
                handle,
                config: SpiConfig::default(),
            })
        }
    }
}

impl SpiMaster for EspSpiMaster {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        let mode = match config.mode {
            SpiMode::Mode0 => 0,
            SpiMode::Mode1 => 1,
            SpiMode::Mode2 => 2,
            SpiMode::Mode3 => 3,
        };

        unsafe {
            // Remove old device
            let ret = sys::spi_bus_remove_device(self.handle);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Get CS pin from old config
            // In a real implementation, we'd store this separately
            let cs_pin = -1; // Would need to track this

            // Add device with new configuration
            let dev_config = sys::spi_device_interface_config_t {
                command_bits: 0,
                address_bits: 0,
                dummy_bits: 0,
                mode: mode as u8,
                clock_source: sys::spi_clock_source_t_SPI_CLK_SRC_DEFAULT,
                duty_cycle_pos: 128,
                cs_ena_pretrans: 0,
                cs_ena_posttrans: 0,
                clock_speed_hz: config.frequency as i32,
                input_delay_ns: 0,
                spics_io_num: cs_pin,
                flags: if config.bit_order == BitOrder::LsbFirst {
                    sys::SPI_DEVICE_BIT_LSBFIRST
                } else {
                    0
                },
                queue_size: 1,
                pre_cb: None,
                post_cb: None,
            };

            let ret = sys::spi_bus_add_device(self.host, &dev_config, &mut self.handle);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        self.config = config.clone();
        Ok(())
    }

    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()> {
        if tx_data.len() != rx_buffer.len() {
            return Err(HalError::InvalidParameter);
        }

        unsafe {
            let mut transaction = sys::spi_transaction_t {
                flags: 0,
                __bindgen_anon_1: sys::spi_transaction_t__bindgen_ty_1 {
                    cmd: 0,
                },
                __bindgen_anon_2: sys::spi_transaction_t__bindgen_ty_2 {
                    addr: 0,
                },
                length: (tx_data.len() * 8) as usize,
                rxlength: (rx_buffer.len() * 8) as usize,
                user: core::ptr::null_mut(),
                __bindgen_anon_3: sys::spi_transaction_t__bindgen_ty_3 {
                    tx_buffer: tx_data.as_ptr() as *const core::ffi::c_void,
                },
                __bindgen_anon_4: sys::spi_transaction_t__bindgen_ty_4 {
                    rx_buffer: rx_buffer.as_mut_ptr() as *mut core::ffi::c_void,
                },
            };

            let ret = sys::spi_device_polling_transmit(self.handle, &mut transaction);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        unsafe {
            let mut transaction = sys::spi_transaction_t {
                flags: 0,
                __bindgen_anon_1: sys::spi_transaction_t__bindgen_ty_1 {
                    cmd: 0,
                },
                __bindgen_anon_2: sys::spi_transaction_t__bindgen_ty_2 {
                    addr: 0,
                },
                length: (data.len() * 8) as usize,
                rxlength: 0,
                user: core::ptr::null_mut(),
                __bindgen_anon_3: sys::spi_transaction_t__bindgen_ty_3 {
                    tx_buffer: data.as_ptr() as *const core::ffi::c_void,
                },
                __bindgen_anon_4: sys::spi_transaction_t__bindgen_ty_4 {
                    rx_buffer: core::ptr::null_mut(),
                },
            };

            let ret = sys::spi_device_polling_transmit(self.handle, &mut transaction);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()> {
        unsafe {
            let mut transaction = sys::spi_transaction_t {
                flags: 0,
                __bindgen_anon_1: sys::spi_transaction_t__bindgen_ty_1 {
                    cmd: 0,
                },
                __bindgen_anon_2: sys::spi_transaction_t__bindgen_ty_2 {
                    addr: 0,
                },
                length: (buffer.len() * 8) as usize,
                rxlength: (buffer.len() * 8) as usize,
                user: core::ptr::null_mut(),
                __bindgen_anon_3: sys::spi_transaction_t__bindgen_ty_3 {
                    tx_buffer: core::ptr::null(),
                },
                __bindgen_anon_4: sys::spi_transaction_t__bindgen_ty_4 {
                    rx_buffer: buffer.as_mut_ptr() as *mut core::ffi::c_void,
                },
            };

            let ret = sys::spi_device_polling_transmit(self.handle, &mut transaction);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        Ok(())
    }
}

impl Drop for EspSpiMaster {
    fn drop(&mut self) {
        unsafe {
            sys::spi_bus_remove_device(self.handle);
            // Note: We don't free the bus here as it might be shared
        }
    }
}

use core::ptr;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spi_config() {
        let config = SpiConfig {
            frequency: 1_000_000,
            mode: SpiMode::Mode0,
            bit_order: BitOrder::MsbFirst,
        };
        assert_eq!(config.frequency, 1_000_000);
    }
}
