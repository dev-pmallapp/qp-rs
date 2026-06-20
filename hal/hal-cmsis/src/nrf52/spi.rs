//! nRF52840 SPIM driver

use hal::spi::{SpiMaster, SpiConfig, SpiMode, BitOrder};
use hal::error::HalResult;
use super::regs::SpiRegs;

/// nRF52 SPIM Master implementation
pub struct Nrf52Spi {
    regs: *const SpiRegs,
}

unsafe impl Send for Nrf52Spi {}
unsafe impl Sync for Nrf52Spi {}

impl Nrf52Spi {
    /// Create a new Nrf52Spi handle
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

#[allow(deprecated)]
impl SpiMaster for Nrf52Spi {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        // Set frequency register values (constants from Product Specification)
        let freq = match config.frequency {
            f if f >= 8_000_000 => 0x08000000, // 8 Mbps
            f if f >= 4_000_000 => 0x04000000, // 4 Mbps
            f if f >= 2_000_000 => 0x02000000, // 2 Mbps
            _                   => 0x01800000, // 1 Mbps (default)
        };
        self.regs().frequency.write(freq);

        // Set configuration (CPOL, CPHA, Bit Order)
        let mut cfg = 0;
        match config.mode {
            SpiMode::Mode0 => {}
            SpiMode::Mode1 => cfg |= 1 << 0, // CPHA
            SpiMode::Mode2 => cfg |= 1 << 1, // CPOL
            SpiMode::Mode3 => cfg |= (1 << 1) | (1 << 0), // CPOL | CPHA
        }

        if config.bit_order == BitOrder::LsbFirst {
            cfg |= 1 << 2; // LSB first
        }
        self.regs().config.write(cfg);

        // Enable SPIM (enable value = 7)
        self.regs().enable.write(7);
        Ok(())
    }

    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()> {
        let len = tx_data.len().min(rx_buffer.len());
        if len == 0 {
            return Ok(());
        }

        self.regs().txd.ptr.write(tx_data.as_ptr() as u32);
        self.regs().txd.maxcnt.write(len as u32);
        self.regs().rxd.ptr.write(rx_buffer.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(len as u32);

        // Clear events_ready
        self.regs().events_ready.write(0);
        // Start SPIM transfer
        self.regs().tasks_start.write(1);

        // Wait until Ready event is set
        while self.regs().events_ready.read() == 0 {}

        // Stop SPIM
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        if data.is_empty() {
            return Ok(());
        }

        self.regs().txd.ptr.write(data.as_ptr() as u32);
        self.regs().txd.maxcnt.write(data.len() as u32);
        self.regs().rxd.ptr.write(0);
        self.regs().rxd.maxcnt.write(0);

        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        self.regs().txd.ptr.write(0);
        self.regs().txd.maxcnt.write(0);
        self.regs().rxd.ptr.write(buffer.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(buffer.len() as u32);

        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 SpiBus impl
// ---------------------------------------------------------------------------
impl embedded_hal::spi::ErrorType for Nrf52Spi {
    type Error = hal::error::HalError;
}

impl embedded_hal::spi::SpiBus<u8> for Nrf52Spi {
    fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        if words.is_empty() { return Ok(()); }
        self.regs().txd.ptr.write(0);
        self.regs().txd.maxcnt.write(0);
        self.regs().rxd.ptr.write(words.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(words.len() as u32);
        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        if words.is_empty() { return Ok(()); }
        self.regs().txd.ptr.write(words.as_ptr() as u32);
        self.regs().txd.maxcnt.write(words.len() as u32);
        self.regs().rxd.ptr.write(0);
        self.regs().rxd.maxcnt.write(0);
        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        let len = read.len().min(write.len());
        if len == 0 { return Ok(()); }
        self.regs().txd.ptr.write(write.as_ptr() as u32);
        self.regs().txd.maxcnt.write(len as u32);
        self.regs().rxd.ptr.write(read.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(len as u32);
        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        // nRF52 SPIM can use same buffer for TX and RX (EasyDMA allows overlap)
        if words.is_empty() { return Ok(()); }
        self.regs().txd.ptr.write(words.as_ptr() as u32);
        self.regs().txd.maxcnt.write(words.len() as u32);
        self.regs().rxd.ptr.write(words.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(words.len() as u32);
        self.regs().events_ready.write(0);
        self.regs().tasks_start.write(1);
        while self.regs().events_ready.read() == 0 {}
        self.regs().tasks_stop.write(1);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // SPIM is always flushed after tasks_stop; nothing to do.
        Ok(())
    }
}
