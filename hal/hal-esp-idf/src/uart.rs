//! UART implementation via ESP-IDF `uart_*` API.

use hal::error::{HalError, HalResult};
use hal::uart::{DataBits, Parity, StopBits, UartConfig, UartPort};

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
use esp_idf_sys as sys;

pub struct EspUart {
    port: i32,
}

impl EspUart {
    pub fn new(port: u8) -> HalResult<Self> {
        if port > 2 {
            return Err(HalError::InvalidParameter);
        }
        Ok(Self { port: port as i32 })
    }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl UartPort for EspUart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        let data_bits = match config.data_bits {
            DataBits::Five  => sys::uart_word_length_t_UART_DATA_5_BITS,
            DataBits::Six   => sys::uart_word_length_t_UART_DATA_6_BITS,
            DataBits::Seven => sys::uart_word_length_t_UART_DATA_7_BITS,
            DataBits::Eight => sys::uart_word_length_t_UART_DATA_8_BITS,
        };
        let parity = match config.parity {
            Parity::None => sys::uart_parity_t_UART_PARITY_DISABLE,
            Parity::Even => sys::uart_parity_t_UART_PARITY_EVEN,
            Parity::Odd  => sys::uart_parity_t_UART_PARITY_ODD,
        };
        let stop_bits = match config.stop_bits {
            StopBits::One => sys::uart_stop_bits_t_UART_STOP_BITS_1,
            StopBits::Two => sys::uart_stop_bits_t_UART_STOP_BITS_2,
        };
        let uart_cfg = sys::uart_config_t {
            baud_rate: config.baud_rate as i32,
            data_bits,
            parity,
            stop_bits,
            flow_ctrl: sys::uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
            source_clk: sys::uart_sclk_t_UART_SCLK_APB,
            ..Default::default()
        };
        let ret = unsafe { sys::uart_param_config(self.port, &uart_cfg) };
        if ret != sys::ESP_OK as i32 {
            return Err(HalError::VendorError(ret));
        }
        // Install driver with 256-byte RX and TX buffers
        let ret = unsafe {
            sys::uart_driver_install(self.port, 256, 256, 0, core::ptr::null_mut(), 0)
        };
        if ret != sys::ESP_OK as i32 {
            Err(HalError::VendorError(ret))
        } else {
            Ok(())
        }
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        let written = unsafe {
            sys::uart_write_bytes(self.port, data.as_ptr() as *const i8, data.len())
        };
        if written < 0 {
            Err(HalError::HardwareError)
        } else {
            Ok(written as usize)
        }
    }

    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        let ticks = (timeout_ms as i32) / 10; // portTICK_PERIOD_MS ≈ 10 ms
        let n = unsafe {
            sys::uart_read_bytes(
                self.port,
                buffer.as_mut_ptr() as *mut core::ffi::c_void,
                buffer.len() as u32,
                ticks,
            )
        };
        if n < 0 {
            Err(HalError::HardwareError)
        } else {
            Ok(n as usize)
        }
    }

    fn available(&self) -> usize {
        let mut len: usize = 0;
        unsafe { sys::uart_get_buffered_data_len(self.port, &mut len) };
        len
    }

    fn flush(&mut self) -> HalResult<()> {
        let ret = unsafe { sys::uart_flush(self.port) };
        if ret != sys::ESP_OK as i32 {
            Err(HalError::VendorError(ret))
        } else {
            Ok(())
        }
    }
}

#[cfg(any(feature = "esp32", feature = "esp32s3", feature = "esp32c6"))]
impl Drop for EspUart {
    fn drop(&mut self) {
        unsafe { sys::uart_driver_delete(self.port); }
    }
}
