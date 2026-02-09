//! ESP32 UART implementation using ESP-IDF

use hal::uart::{DataBits, FlowControl, Parity, StopBits, UartConfig, UartPort};
use hal::error::{HalError, HalResult};
use esp_idf_sys as sys;
use core::ptr;

/// ESP32 UART port implementation
pub struct EspUart {
    port: i32,
    config: UartConfig,
}

impl EspUart {
    /// Create a new UART port
    ///
    /// # Arguments
    /// * `port` - UART port number (0-2 on most ESP32 chips)
    ///
    /// # Note
    /// The UART is not configured until `configure()` is called.
    pub fn new(port: u8) -> HalResult<Self> {
        // ESP32 typically has UART0, UART1, UART2
        if port > 2 {
            return Err(HalError::InvalidParameter);
        }

        Ok(Self {
            port: port as i32,
            config: UartConfig::default(),
        })
    }

    /// Get the port number
    pub fn port_number(&self) -> u8 {
        self.port as u8
    }
}

impl UartPort for EspUart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        let data_bits = match config.data_bits {
            DataBits::Five => sys::uart_word_length_t_UART_DATA_5_BITS,
            DataBits::Six => sys::uart_word_length_t_UART_DATA_6_BITS,
            DataBits::Seven => sys::uart_word_length_t_UART_DATA_7_BITS,
            DataBits::Eight => sys::uart_word_length_t_UART_DATA_8_BITS,
        };

        let parity = match config.parity {
            Parity::None => sys::uart_parity_t_UART_PARITY_DISABLE,
            Parity::Even => sys::uart_parity_t_UART_PARITY_EVEN,
            Parity::Odd => sys::uart_parity_t_UART_PARITY_ODD,
        };

        let stop_bits = match config.stop_bits {
            StopBits::One => sys::uart_stop_bits_t_UART_STOP_BITS_1,
            StopBits::Two => sys::uart_stop_bits_t_UART_STOP_BITS_2,
        };

        let flow_ctrl = match config.flow_control {
            FlowControl::None => sys::uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
            FlowControl::RtsCts => sys::uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_CTS_RTS,
        };

        unsafe {
            // Configure UART parameters
            let uart_config = sys::uart_config_t {
                baud_rate: config.baud_rate as i32,
                data_bits,
                parity,
                stop_bits,
                flow_ctrl,
                rx_flow_ctrl_thresh: 122, // Default threshold
                __bindgen_anon_1: sys::uart_config_t__bindgen_ty_1 { flags: 0 },
            };

            let ret = sys::uart_param_config(self.port, &uart_config);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Set default pins based on port
            // Note: Users can reconfigure pins if needed
            let (tx_pin, rx_pin, rts_pin, cts_pin) = match self.port {
                0 => (1, 3, -1, -1),   // UART0: TX=GPIO1, RX=GPIO3
                1 => (10, 9, -1, -1),  // UART1: TX=GPIO10, RX=GPIO9
                2 => (17, 16, -1, -1), // UART2: TX=GPIO17, RX=GPIO16
                _ => return Err(HalError::InvalidParameter),
            };

            let ret = sys::uart_set_pin(self.port, tx_pin, rx_pin, rts_pin, cts_pin);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }

            // Install UART driver with RX buffer
            // TX buffer = 0 (synchronous), RX buffer = 1024 bytes
            let ret = sys::uart_driver_install(
                self.port,
                1024, // RX buffer size
                0,    // TX buffer size (0 = synchronous)
                0,    // Event queue size (0 = no queue)
                ptr::null_mut(),
                0, // Interrupt flags
            );
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }

        self.config = config.clone();
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        unsafe {
            let written = sys::uart_write_bytes(
                self.port,
                data.as_ptr() as *const i8,
                data.len() as u32,
            );
            if written < 0 {
                Err(HalError::HardwareError)
            } else {
                Ok(written as usize)
            }
        }
    }

    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        unsafe {
            // Convert milliseconds to ticks
            let ticks = if timeout_ms == 0 {
                0
            } else {
                (timeout_ms as u32 * 1000) / sys::portTICK_PERIOD_MS
            };

            let read = sys::uart_read_bytes(
                self.port,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                ticks,
            );

            if read < 0 {
                Err(HalError::Timeout)
            } else {
                Ok(read as usize)
            }
        }
    }

    fn available(&self) -> usize {
        unsafe {
            let mut size: usize = 0;
            sys::uart_get_buffered_data_len(self.port, &mut size);
            size
        }
    }

    fn flush(&mut self) -> HalResult<()> {
        unsafe {
            let ret = sys::uart_flush(self.port);
            if ret != sys::ESP_OK as i32 {
                return Err(HalError::VendorError(ret));
            }
        }
        Ok(())
    }
}

impl Drop for EspUart {
    fn drop(&mut self) {
        // Clean up: delete driver on drop
        unsafe {
            sys::uart_driver_delete(self.port);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uart_creation() {
        let uart = EspUart::new(0);
        assert!(uart.is_ok());
        assert_eq!(uart.unwrap().port_number(), 0);
    }

    #[test]
    fn test_invalid_port() {
        let uart = EspUart::new(10);
        assert!(uart.is_err());
    }

    #[test]
    fn test_default_config() {
        let uart = EspUart::new(1).unwrap();
        assert_eq!(uart.config.baud_rate, 115200);
        assert_eq!(uart.config.data_bits, DataBits::Eight);
    }
}
