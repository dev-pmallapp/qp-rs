//! LPC1768 SSP (SPI) driver

use hal::spi::{BitOrder, SpiConfig, SpiMode};
use hal::error::{HalError, HalResult};
use super::regs::{SspRegs, SSP_CR1_SSE, SSP_SR_BSY, SSP_SR_RNE, SSP_SR_TNF};

// Assume 25 MHz PCLK (LPC1768 default after reset with PLL = 100 MHz / 4)
const PCLK_HZ: u32 = 25_000_000;

/// LPC1768 SSP (SPI) Master.
pub struct Lpc17Spi {
    regs: *const SspRegs,
}

unsafe impl Send for Lpc17Spi {}
unsafe impl Sync for Lpc17Spi {}

impl Lpc17Spi {
    /// Create a new SSP handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of the SSP peripheral.
    pub unsafe fn new(regs: *const SspRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &SspRegs {
        unsafe { &*self.regs }
    }

    fn transfer_byte(&self, tx: u8) -> u8 {
        // Wait until TX FIFO has room
        while (self.regs().sr.read() & SSP_SR_TNF) == 0 {}
        self.regs().dr.write(tx as u32);
        // Wait until RX FIFO has data
        while (self.regs().sr.read() & SSP_SR_RNE) == 0 {}
        self.regs().dr.read() as u8
    }

    fn wait_idle(&self) {
        while (self.regs().sr.read() & SSP_SR_BSY) != 0 {}
    }
}

impl Lpc17Spi {
    /// Configure SSP clock, mode and bit order (embedded-hal `SpiBus` has no
    /// configure step).
    pub fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        // Disable SSP during configuration
        self.regs().cr1.write(0);

        // CR0: 8-bit data, SPI frame, CPOL/CPHA from mode, SCR for baud
        let (cpol, cpha) = match config.mode {
            SpiMode::Mode0 => (0u32, 0u32),
            SpiMode::Mode1 => (0, 1),
            SpiMode::Mode2 => (1, 0),
            SpiMode::Mode3 => (1, 1),
        };
        // Clock rate: FSSP = PCLK / (CPSDVSR * (1 + SCR))
        // Use CPSR = 2 (minimum) and compute SCR
        let cpsr: u32 = 2;
        let scr = (PCLK_HZ / (cpsr * config.frequency)).saturating_sub(1).min(255);
        let lsbfirst = if config.bit_order == BitOrder::LsbFirst { 1u32 } else { 0 };
        let cr0 = 0x7               // DSS = 8-bit (0111)
            | (0 << 4)              // FRF = SPI
            | (cpol << 6)
            | (cpha << 7)
            | (scr << 8)
            | (lsbfirst << 16);    // non-standard; some LPC variants support it in CR1

        if config.frequency == 0 {
            return Err(HalError::InvalidParameter);
        }

        self.regs().cpsr.write(cpsr);
        self.regs().cr0.write(cr0);
        // Enable SSP as master
        self.regs().cr1.write(SSP_CR1_SSE);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 SpiBus impl
// ---------------------------------------------------------------------------
impl embedded_hal::spi::ErrorType for Lpc17Spi {
    type Error = hal::error::HalError;
}

impl embedded_hal::spi::SpiBus<u8> for Lpc17Spi {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for slot in words.iter_mut() {
            *slot = self.transfer_byte(0xFF);
        }
        self.wait_idle();
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        for &b in words {
            self.transfer_byte(b);
        }
        self.wait_idle();
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        let len = read.len().min(write.len());
        for i in 0..len {
            read[i] = self.transfer_byte(write[i]);
        }
        self.wait_idle();
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        for slot in words.iter_mut() {
            *slot = self.transfer_byte(*slot);
        }
        self.wait_idle();
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.wait_idle();
        Ok(())
    }
}
