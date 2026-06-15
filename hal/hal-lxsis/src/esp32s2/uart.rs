//! ESP32-S2 UART driver

use hal::uart::{DataBits, Parity, StopBits, UartConfig, UartPort};
use hal::error::HalResult;
use super::regs::UartRegs;

/// ESP32-S2 UART port.
pub struct Esp32S2Uart {
    regs: *const UartRegs,
}

unsafe impl Send for Esp32S2Uart {}
unsafe impl Sync for Esp32S2Uart {}

impl Esp32S2Uart {
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

impl UartPort for Esp32S2Uart {
    fn configure(&mut self, config: &UartConfig) -> HalResult<()> {
        // Baud rate using APB clock (80 MHz)
        let clk_div = 80_000_000 / config.baud_rate;
        let clk_div_frac = ((80_000_000 % config.baud_rate) * 16) / config.baud_rate;
        self.regs().clkdiv.write(clk_div | (clk_div_frac << 20));

        let data_bits_val = match config.data_bits {
            DataBits::Five  => 0,
            DataBits::Six   => 1,
            DataBits::Seven => 2,
            DataBits::Eight => 3,
        };
        let mut conf0 = data_bits_val << 2;

        match config.parity {
            Parity::None => {}
            Parity::Even => conf0 |= 1 << 1,
            Parity::Odd  => conf0 |= (1 << 1) | (1 << 0),
        }
        match config.stop_bits {
            StopBits::One => conf0 |= 1 << 4,
            StopBits::Two => conf0 |= 3 << 4,
        }

        // Reset FIFOs then release
        self.regs().conf0.write(conf0 | (1 << 17) | (1 << 18));
        self.regs().conf0.write(conf0);
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        for &byte in data {
            while ((self.regs().status.read() >> 16) & 0xFF) >= 128 {}
            self.regs().fifo.write(byte as u32);
        }
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        let mut count = 0;
        for slot in buffer.iter_mut() {
            let mut timeout = timeout_ms * 1000;
            while (self.regs().status.read() & 0xFF) == 0 {
                if timeout == 0 {
                    return Ok(count);
                }
                timeout -= 1;
                for _ in 0..10 {
                    core::hint::spin_loop();
                }
            }
            *slot = self.regs().fifo.read() as u8;
            count += 1;
        }
        Ok(count)
    }

    fn available(&self) -> usize {
        (self.regs().status.read() & 0xFF) as usize
    }

    fn flush(&mut self) -> HalResult<()> {
        while ((self.regs().status.read() >> 16) & 0xFF) != 0 {}
        Ok(())
    }
}
