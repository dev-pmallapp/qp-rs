//! STM32F4 USART driver

use hal::uart::{UartPort, UartConfig, DataBits, StopBits, Parity};
use hal::error::{HalError, HalResult};
use super::regs::UsartRegs;

/// STM32F4 USART implementation
pub struct Stm32F4Uart {
    regs: *const UsartRegs,
}

unsafe impl Send for Stm32F4Uart {}
unsafe impl Sync for Stm32F4Uart {}

impl Stm32F4Uart {
    /// Create a new Stm32F4Uart handle
    ///
    /// # Safety
    /// Unique ownership of this USART peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const UsartRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &UsartRegs {
        unsafe { &*self.regs }
    }
}

impl UartPort for Stm32F4Uart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        let mut cr1 = (1 << 3) | (1 << 2); // TE (Transmitter enable) | RE (Receiver enable)

        match config.data_bits {
            DataBits::Eight => {}
            _ => return Err(HalError::NotSupported),
        }

        match config.parity {
            Parity::None => {}
            Parity::Even => cr1 |= 1 << 10, // PCE (Parity control enable)
            Parity::Odd  => cr1 |= (1 << 10) | (1 << 9), // PCE | PS (Parity selection odd)
        }

        let mut cr2 = 0;
        match config.stop_bits {
            StopBits::One => {}
            StopBits::Two => cr2 |= 0b10 << 12, // 2 stop bits
        }

        // Baud rate calculation (assume 16 MHz peripheral clock)
        let brr = 16_000_000 / config.baud_rate;
        self.regs().brr.write(brr);
        self.regs().cr2.write(cr2);
        self.regs().cr1.write(cr1);
        self.regs().cr1.modify(|v| v | (1 << 13)); // UE (USART Enable)
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        for &byte in data {
            // Wait until TXE (Transmit data register empty) is set (SR bit 7)
            while (self.regs().sr.read() & (1 << 7)) == 0 {}
            self.regs().dr.write(byte as u32);
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], _timeout_ms: u32) -> HalResult<usize> {
        let mut count = 0;
        for i in 0..buffer.len() {
            let mut timeout = 100_000;
            // Wait until RXNE (Read data register not empty) is set (SR bit 5)
            while (self.regs().sr.read() & (1 << 5)) == 0 {
                timeout -= 1;
                if timeout == 0 {
                    return Ok(count);
                }
            }
            buffer[i] = self.regs().dr.read() as u8;
            count += 1;
        }
        Ok(count)
    }

    fn available(&self) -> usize {
        if (self.regs().sr.read() & (1 << 5)) != 0 {
            1
        } else {
            0
        }
    }

    fn flush(&mut self) -> HalResult<()> {
        // Wait until TC (Transmission complete) is set (SR bit 6)
        while (self.regs().sr.read() & (1 << 6)) == 0 {}
        Ok(())
    }
}
