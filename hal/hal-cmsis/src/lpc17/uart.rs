//! LPC1768 UART driver

use hal::uart::{DataBits, Parity, StopBits, UartConfig, UartPort};
use hal::error::HalResult;
use super::regs::{UartRegs, UART_LCR_DLAB, UART_LSR_RDR, UART_LSR_THRE};

// Assume 25 MHz PCLK
const PCLK_HZ: u32 = 25_000_000;

/// LPC1768 UART port.
pub struct Lpc17Uart {
    regs: *const UartRegs,
}

unsafe impl Send for Lpc17Uart {}
unsafe impl Sync for Lpc17Uart {}

impl Lpc17Uart {
    /// Create a new UART handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of the UART peripheral.
    pub unsafe fn new(regs: *const UartRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &UartRegs {
        unsafe { &*self.regs }
    }
}

#[allow(deprecated)]
impl UartPort for Lpc17Uart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        // Enable FIFO and reset TX/RX FIFOs via FCR
        self.regs().iir_fcr.write(0x07); // FIFO enable, clear RX/TX

        // Build LCR: set DLAB to access DLL/DLM
        let data_bits = match config.data_bits {
            DataBits::Five  => 0u32,
            DataBits::Six   => 1,
            DataBits::Seven => 2,
            DataBits::Eight => 3,
        };
        let stop = match config.stop_bits {
            StopBits::One => 0u32,
            StopBits::Two => 1,
        };
        let (par_en, par_sel) = match config.parity {
            Parity::None => (0u32, 0u32),
            Parity::Odd  => (1, 0),
            Parity::Even => (1, 1),
        };
        let lcr_base = data_bits | (stop << 2) | (par_en << 3) | (par_sel << 4);

        // Set DLAB to configure baud rate divisor
        self.regs().lcr.write(lcr_base | UART_LCR_DLAB);
        let dl = (PCLK_HZ / (16 * config.baud_rate)).max(1);
        self.regs().rbr_thr_dll.write(dl & 0xFF);        // DLL
        self.regs().dlm_ier.write((dl >> 8) & 0xFF);     // DLM

        // Clear DLAB — normal operating mode
        self.regs().lcr.write(lcr_base);
        // Enable TX (TER bit 7)
        self.regs().ter.write(1 << 7);
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        for &byte in data {
            while (self.regs().lsr.read() & UART_LSR_THRE) == 0 {}
            self.regs().rbr_thr_dll.write(byte as u32);
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        let mut count = 0;
        for slot in buffer.iter_mut() {
            let mut timeout = timeout_ms * 1000;
            while (self.regs().lsr.read() & UART_LSR_RDR) == 0 {
                if timeout == 0 {
                    return Ok(count);
                }
                timeout -= 1;
                for _ in 0..10 {
                    core::hint::spin_loop();
                }
            }
            *slot = self.regs().rbr_thr_dll.read() as u8;
            count += 1;
        }
        Ok(count)
    }

    fn available(&self) -> usize {
        usize::from((self.regs().lsr.read() & UART_LSR_RDR) != 0)
    }

    fn flush(&mut self) -> HalResult<()> {
        while (self.regs().lsr.read() & UART_LSR_THRE) == 0 {}
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// embedded-io impls
// ---------------------------------------------------------------------------
impl embedded_io::ErrorType for Lpc17Uart {
    type Error = hal::error::HalError;
}

impl embedded_io::Write for Lpc17Uart {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &byte in buf {
            while (self.regs().lsr.read() & UART_LSR_THRE) == 0 {}
            self.regs().rbr_thr_dll.write(byte as u32);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        while (self.regs().lsr.read() & UART_LSR_THRE) == 0 {}
        Ok(())
    }
}

impl embedded_io::Read for Lpc17Uart {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() { return Ok(0); }
        // Block for first byte
        while (self.regs().lsr.read() & UART_LSR_RDR) == 0 {}
        let mut count = 0;
        for slot in buf.iter_mut() {
            if (self.regs().lsr.read() & UART_LSR_RDR) == 0 {
                break;
            }
            *slot = self.regs().rbr_thr_dll.read() as u8;
            count += 1;
        }
        Ok(count)
    }
}
