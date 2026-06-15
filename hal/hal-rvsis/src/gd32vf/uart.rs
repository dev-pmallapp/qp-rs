//! GD32VF103 USART driver

use hal::uart::{UartPort, UartConfig, DataBits, StopBits, Parity};
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
}

impl UartPort for Gd32VfUart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
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

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        for &byte in data {
            // Wait until TBE (Transmit data register empty) is set (STAT bit 7)
            while (self.regs().stat.read() & (1 << 7)) == 0 {}
            self.regs().data.write(byte as u32);
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], _timeout_ms: u32) -> HalResult<usize> {
        let mut count = 0;
        for i in 0..buffer.len() {
            let mut timeout = 100_000;
            // Wait until RBNE (Read data register not empty) is set (STAT bit 5)
            while (self.regs().stat.read() & (1 << 5)) == 0 {
                timeout -= 1;
                if timeout == 0 {
                    return Ok(count);
                }
            }
            buffer[i] = self.regs().data.read() as u8;
            count += 1;
        }
        Ok(count)
    }

    fn available(&self) -> usize {
        if (self.regs().stat.read() & (1 << 5)) != 0 {
            1
        } else {
            0
        }
    }

    fn flush(&mut self) -> HalResult<()> {
        // Wait until TC (Transmission complete) is set (STAT bit 6)
        while (self.regs().stat.read() & (1 << 6)) == 0 {}
        Ok(())
    }
}
