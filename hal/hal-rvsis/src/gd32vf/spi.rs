//! GD32VF103 SPI driver

use hal::spi::{SpiMaster, SpiConfig, SpiMode, BitOrder};
use hal::error::HalResult;
use super::regs::SpiRegs;

/// GD32VF103 SPI Master
pub struct Gd32VfSpi {
    regs: *const SpiRegs,
}

unsafe impl Send for Gd32VfSpi {}
unsafe impl Sync for Gd32VfSpi {}

impl Gd32VfSpi {
    /// Create a new Gd32VfSpi handle
    ///
    /// # Safety
    /// Unique ownership of the SPI peripheral must be guaranteed by the caller.
    pub unsafe fn new(regs: *const SpiRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &SpiRegs {
        unsafe { &*self.regs }
    }
}

impl SpiMaster for Gd32VfSpi {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        let mut ctl0 = 1 << 2; // MSTMOD (Master mode)
        ctl0 |= 1 << 9; // SWNSSEN (Software NSS management)
        ctl0 |= 1 << 8; // SWNSS (Internal NSS select)

        match config.mode {
            SpiMode::Mode0 => {}
            SpiMode::Mode1 => ctl0 |= 1 << 0, // CKPHA
            SpiMode::Mode2 => ctl0 |= 1 << 1, // CKPL
            SpiMode::Mode3 => ctl0 |= (1 << 1) | (1 << 0), // CKPL | CKPHA
        }

        if config.bit_order == BitOrder::LsbFirst {
            ctl0 |= 1 << 7; // LF (LSB first)
        }

        // Simple prescaler mapping based on config.frequency (assume 108 MHz base clock or similar)
        let psc = match config.frequency {
            f if f >= 54_000_000 => 0b000, // fpclk / 2
            f if f >= 27_000_000 => 0b001, // fpclk / 4
            f if f >= 13_500_000 => 0b010, // fpclk / 8
            f if f >= 6_750_000  => 0b011, // fpclk / 16
            f if f >= 3_375_000  => 0b100, // fpclk / 32
            f if f >= 1_687_500  => 0b101, // fpclk / 64
            f if f >= 843_750    => 0b110, // fpclk / 128
            _                    => 0b111, // fpclk / 256
        };
        ctl0 |= psc << 3;

        self.regs().ctl0.write(ctl0);
        self.regs().ctl0.modify(|v| v | (1 << 6)); // SPIEN (SPI enable)
        Ok(())
    }

    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()> {
        let len = tx_data.len().min(rx_buffer.len());
        for i in 0..len {
            // Wait until TBE (Transmit buffer empty) is set (STAT bit 1)
            while (self.regs().stat.read() & (1 << 1)) == 0 {}
            self.regs().data.write(tx_data[i] as u32);

            // Wait until RBNE (Receive buffer not empty) is set (STAT bit 0)
            while (self.regs().stat.read() & (1 << 0)) == 0 {}
            rx_buffer[i] = self.regs().data.read() as u8;
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        for &byte in data {
            // Wait until TBE is set
            while (self.regs().stat.read() & (1 << 1)) == 0 {}
            self.regs().data.write(byte as u32);

            // Wait until RBNE is set and read data to clear flags
            while (self.regs().stat.read() & (1 << 0)) == 0 {}
            let _ = self.regs().data.read();
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()> {
        for i in 0..buffer.len() {
            // Wait until TBE is set
            while (self.regs().stat.read() & (1 << 1)) == 0 {}
            self.regs().data.write(0xFF); // Send dummy byte

            // Wait until RBNE is set
            while (self.regs().stat.read() & (1 << 0)) == 0 {}
            buffer[i] = self.regs().data.read() as u8;
        }
        Ok(())
    }
}
