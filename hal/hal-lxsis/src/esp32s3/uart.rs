//! ESP32-S3 UART driver

use hal::uart::{UartConfig, DataBits, StopBits, Parity};
use hal::error::{HalError, HalResult};
use super::regs::UartRegs;

/// ESP32-S3 UART Port
pub struct Esp32S3Uart {
    regs: *const UartRegs,
}

unsafe impl Send for Esp32S3Uart {}
unsafe impl Sync for Esp32S3Uart {}

impl Esp32S3Uart {
    /// Create a new Esp32S3Uart handle
    ///
    /// # Safety
    /// Unique ownership of the UART peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const UartRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &UartRegs {
        unsafe { &*self.regs }
    }

    /// Configure baud rate, framing and parity. Call once before use
    /// (embedded-io `Read`/`Write` have no configure step).
    pub fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        // Baud rate calculation using APB clock (80 MHz)
        let clk_div = 80_000_000 / config.baud_rate;
        let clk_div_frac = ((80_000_000 % config.baud_rate) * 16) / config.baud_rate;
        self.regs().clkdiv.write(clk_div | (clk_div_frac << 20));

        let data_bits_val = match config.data_bits {
            DataBits::Five => 0,
            DataBits::Six => 1,
            DataBits::Seven => 2,
            DataBits::Eight => 3,
        };

        let mut conf0 = data_bits_val << 2;

        match config.parity {
            Parity::None => {}
            Parity::Even => conf0 |= 1 << 1,
            Parity::Odd => conf0 |= (1 << 1) | (1 << 0),
        }

        match config.stop_bits {
            StopBits::One => conf0 |= 1 << 4,
            StopBits::Two => conf0 |= 3 << 4,
        }

        // Reset RX/TX FIFOs (bits 17 and 18)
        self.regs().conf0.write(conf0 | (1 << 17) | (1 << 18));
        self.regs().conf0.write(conf0); // Release reset

        Ok(())
    }

    /// Number of bytes currently waiting in the RX FIFO.
    pub fn available(&self) -> usize {
        (self.regs().status.read() & 0xFF) as usize
    }
}

impl embedded_io::ErrorType for Esp32S3Uart {
    type Error = HalError;
}

impl embedded_io::Write for Esp32S3Uart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &byte in buf {
            // Wait until TX FIFO has room (TXFIFO_CNT in status bits 16-23 < 128)
            while ((self.regs().status.read() >> 16) & 0xFF) >= 128 {}
            self.regs().fifo.write(byte as u32);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // Wait until TX FIFO is empty (TXFIFO_CNT in status bits 16-23 is 0)
        while ((self.regs().status.read() >> 16) & 0xFF) != 0 {}
        Ok(())
    }
}

impl embedded_io::Read for Esp32S3Uart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        // embedded-io contract: block until at least one byte is available.
        while (self.regs().status.read() & 0xFF) == 0 {
            core::hint::spin_loop();
        }
        let mut count = 0;
        while count < buf.len() && (self.regs().status.read() & 0xFF) != 0 {
            buf[count] = self.regs().fifo.read() as u8;
            count += 1;
        }
        Ok(count)
    }
}
