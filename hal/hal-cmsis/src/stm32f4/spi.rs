//! STM32F4 SPI driver

use hal::spi::{SpiMaster, SpiConfig, SpiMode, BitOrder};
use hal::error::HalResult;
use super::regs::SpiRegs;

/// STM32F4 SPI Master implementation
pub struct Stm32F4Spi {
    regs: *const SpiRegs,
}

unsafe impl Send for Stm32F4Spi {}
unsafe impl Sync for Stm32F4Spi {}

impl Stm32F4Spi {
    /// Create a new Stm32F4Spi handle
    ///
    /// # Safety
    /// Unique ownership of this SPI peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const SpiRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &SpiRegs {
        unsafe { &*self.regs }
    }
}

impl SpiMaster for Stm32F4Spi {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        let mut cr1 = 1 << 2; // MSTR (Master configuration)
        cr1 |= 1 << 9; // SSM (Software slave management)
        cr1 |= 1 << 8; // SSI (Internal slave select)

        match config.mode {
            SpiMode::Mode0 => {}
            SpiMode::Mode1 => cr1 |= 1 << 0, // CPHA
            SpiMode::Mode2 => cr1 |= 1 << 1, // CPOL
            SpiMode::Mode3 => cr1 |= (1 << 1) | (1 << 0), // CPOL | CPHA
        }

        if config.bit_order == BitOrder::LsbFirst {
            cr1 |= 1 << 7; // LSBFIRST
        }

        // Simple prescaler mapping based on config.frequency (assume 16 MHz base clock)
        let br = match config.frequency {
            f if f >= 8_000_000 => 0b000, // f_PCLK / 2
            f if f >= 4_000_000 => 0b001, // f_PCLK / 4
            f if f >= 2_000_000 => 0b010, // f_PCLK / 8
            f if f >= 1_000_000 => 0b011, // f_PCLK / 16
            f if f >= 500_000   => 0b100, // f_PCLK / 32
            f if f >= 250_000   => 0b101, // f_PCLK / 64
            f if f >= 125_000   => 0b110, // f_PCLK / 128
            _                   => 0b111, // f_PCLK / 256
        };
        cr1 |= br << 3;

        self.regs().cr1.write(cr1);
        self.regs().cr1.modify(|v| v | (1 << 6)); // SPE (SPI Enable)
        Ok(())
    }

    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()> {
        let len = tx_data.len().min(rx_buffer.len());
        for i in 0..len {
            // Wait until TXE (Transmit buffer empty) is set (SR bit 1)
            while (self.regs().sr.read() & (1 << 1)) == 0 {}
            self.regs().dr.write(tx_data[i] as u32);

            // Wait until RXNE (Receive buffer not empty) is set (SR bit 0)
            while (self.regs().sr.read() & (1 << 0)) == 0 {}
            rx_buffer[i] = self.regs().dr.read() as u8;
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        for &byte in data {
            // Wait until TXE is set
            while (self.regs().sr.read() & (1 << 1)) == 0 {}
            self.regs().dr.write(byte as u32);
            
            // Wait until RXNE is set and read DR to clear flags
            while (self.regs().sr.read() & (1 << 0)) == 0 {}
            let _ = self.regs().dr.read();
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()> {
        for i in 0..buffer.len() {
            // Wait until TXE is set
            while (self.regs().sr.read() & (1 << 1)) == 0 {}
            self.regs().dr.write(0xFF); // Send dummy byte
            
            // Wait until RXNE is set
            while (self.regs().sr.read() & (1 << 0)) == 0 {}
            buffer[i] = self.regs().dr.read() as u8;
        }
        Ok(())
    }
}
