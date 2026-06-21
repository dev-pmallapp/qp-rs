//! STM32F4 SPI driver

use hal::spi::{SpiConfig, SpiMode, BitOrder};
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

    /// Configure SPI registers from a [`SpiConfig`].
    /// Called by both the legacy trait impl and the `configure()` extension method.
    pub fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
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

    fn transfer_word(&mut self, tx: u8) -> u8 {
        while (self.regs().sr.read() & (1 << 1)) == 0 {} // wait TXE
        self.regs().dr.write(tx as u32);
        while (self.regs().sr.read() & (1 << 0)) == 0 {} // wait RXNE
        self.regs().dr.read() as u8
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 SpiBus impl
// ---------------------------------------------------------------------------
impl embedded_hal::spi::ErrorType for Stm32F4Spi {
    type Error = hal::error::HalError;
}

impl embedded_hal::spi::SpiBus<u8> for Stm32F4Spi {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for slot in words.iter_mut() {
            *slot = self.transfer_word(0xFF);
        }
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        for &byte in words {
            self.transfer_word(byte);
        }
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        let len = read.len().min(write.len());
        for i in 0..len {
            read[i] = self.transfer_word(write[i]);
        }
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for slot in words.iter_mut() {
            *slot = self.transfer_word(*slot);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // Wait until BSY flag (bit 7) is cleared
        while (self.regs().sr.read() & (1 << 7)) != 0 {}
        Ok(())
    }
}
