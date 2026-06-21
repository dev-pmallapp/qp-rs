//! STM32F4 USART driver

use hal::uart::{UartConfig, DataBits, StopBits, Parity};
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

impl Stm32F4Uart {
    /// Configure baud rate, framing and parity (embedded-io `Read`/`Write`
    /// have no configure step).
    pub fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
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

    /// Number of bytes available to read (0 or 1 for this peripheral).
    pub fn available(&self) -> usize {
        if (self.regs().sr.read() & (1 << 5)) != 0 {
            1
        } else {
            0
        }
    }
}

// ---------------------------------------------------------------------------
// embedded-io impls
// ---------------------------------------------------------------------------
impl embedded_io::ErrorType for Stm32F4Uart {
    type Error = hal::error::HalError;
}

impl embedded_io::Write for Stm32F4Uart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &byte in buf {
            // Wait until TXE (SR bit 7)
            while (self.regs().sr.read() & (1 << 7)) == 0 {}
            self.regs().dr.write(byte as u32);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // Wait until TC (Transmission complete, SR bit 6)
        while (self.regs().sr.read() & (1 << 6)) == 0 {}
        Ok(())
    }
}

impl embedded_io::Read for Stm32F4Uart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() { return Ok(0); }
        // Block until at least one byte is available (RXNE, SR bit 5)
        while (self.regs().sr.read() & (1 << 5)) == 0 {}
        let mut count = 0;
        for slot in buf.iter_mut() {
            if (self.regs().sr.read() & (1 << 5)) == 0 {
                break; // no more data immediately available
            }
            *slot = self.regs().dr.read() as u8;
            count += 1;
        }
        Ok(count)
    }
}
