//! GD32VF103 USART driver

use hal::uart::{UartConfig, DataBits, StopBits, Parity};
use hal::error::{HalError, HalResult};
use super::regs::UsartRegs;

/// GD32VF103 USART Port
pub struct Gd32VfUart {
    regs: *const UsartRegs,
}

unsafe impl Send for Gd32VfUart {}
unsafe impl Sync for Gd32VfUart {}

impl Gd32VfUart {
    /// Create a new Gd32VfUart handle
    ///
    /// # Safety
    /// Unique ownership of the USART peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const UsartRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &UsartRegs {
        unsafe { &*self.regs }
    }

    /// Configure baud rate, framing and parity. Call once before use
    /// (embedded-io `Read`/`Write` have no configure step).
    pub fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        let mut ctl0 = (1 << 3) | (1 << 2); // TEN (Transmitter enable) | REN (Receiver enable)

        match config.data_bits {
            DataBits::Eight => {}
            _ => return Err(HalError::NotSupported),
        }

        match config.parity {
            Parity::None => {}
            Parity::Even => ctl0 |= 1 << 10, // PCEN (Parity control enable)
            Parity::Odd  => ctl0 |= (1 << 10) | (1 << 9), // PCEN | PM (Parity selection odd)
        }

        let mut ctl1 = 0;
        match config.stop_bits {
            StopBits::One => {}
            StopBits::Two => ctl1 |= 0b10 << 12, // 2 stop bits (STB = 10)
        }

        // Baud rate calculation (assume 108 MHz system clock)
        let baud_val = 108_000_000 / config.baud_rate;
        self.regs().baud.write(baud_val);
        self.regs().ctl1.write(ctl1);
        self.regs().ctl0.write(ctl0);
        self.regs().ctl0.modify(|v| v | (1 << 13)); // UEN (USART Enable)
        Ok(())
    }

    /// Number of bytes currently available in the RX register (0 or 1).
    pub fn available(&self) -> usize {
        if (self.regs().stat.read() & (1 << 5)) != 0 {
            1
        } else {
            0
        }
    }
}

impl embedded_io::ErrorType for Gd32VfUart {
    type Error = HalError;
}

impl embedded_io::Write for Gd32VfUart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &byte in buf {
            // Wait until TBE (Transmit data register empty) is set (STAT bit 7)
            while (self.regs().stat.read() & (1 << 7)) == 0 {}
            self.regs().data.write(byte as u32);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // Wait until TC (Transmission complete) is set (STAT bit 6)
        while (self.regs().stat.read() & (1 << 6)) == 0 {}
        Ok(())
    }
}

impl embedded_io::Read for Gd32VfUart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        // embedded-io contract: block until at least one byte is available.
        while (self.regs().stat.read() & (1 << 5)) == 0 {
            core::hint::spin_loop();
        }
        let mut count = 0;
        while count < buf.len() && (self.regs().stat.read() & (1 << 5)) != 0 {
            buf[count] = self.regs().data.read() as u8;
            count += 1;
        }
        Ok(count)
    }
}
