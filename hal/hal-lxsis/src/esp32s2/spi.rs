//! ESP32-S2 SPI driver

use hal::spi::{BitOrder, SpiConfig, SpiMaster, SpiMode};
use hal::error::HalResult;
use super::regs::SpiRegs;

/// ESP32-S2 SPI Master.
pub struct Esp32S2Spi {
    regs: *const SpiRegs,
}

unsafe impl Send for Esp32S2Spi {}
unsafe impl Sync for Esp32S2Spi {}

impl Esp32S2Spi {
    /// Create a new SPI handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of the SPI peripheral.
    pub unsafe fn new(regs: *const SpiRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &SpiRegs {
        unsafe { &*self.regs }
    }
}

impl SpiMaster for Esp32S2Spi {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        // Clock divider from APB (80 MHz)
        let div = (80_000_000 / config.frequency).max(1);
        let clock_val = if div <= 1 {
            1 << 31 // CLK_EQU_SYSCLK
        } else {
            let cnt_n = (div - 1) & 0x3F;
            let cnt_h = ((div / 2) - 1) & 0x3F;
            let cnt_l = cnt_n;
            cnt_l | (cnt_h << 6) | (cnt_n << 12)
        };
        self.regs().clock.write(clock_val);

        let mut ctrl2 = 0u32;
        let mut user = (1 << 27) | (1 << 28); // USR_MOSI | USR_MISO
        match config.mode {
            SpiMode::Mode0 => {}
            SpiMode::Mode1 => { user |= 1 << 29; }
            SpiMode::Mode2 => { ctrl2 |= 1 << 24; user |= 1 << 29; }
            SpiMode::Mode3 => { ctrl2 |= 1 << 24; }
        }
        if config.bit_order == BitOrder::LsbFirst {
            user |= (1 << 16) | (1 << 17);
        }
        self.regs().ctrl2.write(ctrl2);
        self.regs().user.write(user);
        Ok(())
    }

    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()> {
        let total_len = tx_data.len().min(rx_buffer.len());
        let mut offset = 0;
        while offset < total_len {
            let chunk_len = (total_len - offset).min(64);
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(bit_len - 1);
            self.regs().miso_dlen.write(bit_len - 1);
            for i in 0..((chunk_len + 3) / 4) {
                let mut word = 0u32;
                for b in 0..4 {
                    let idx = offset + i * 4 + b;
                    if idx - offset < chunk_len {
                        word |= (tx_data[idx] as u32) << (b * 8);
                    }
                }
                self.regs().w[i].write(word);
            }
            self.regs().cmd.modify(|v| v | (1 << 18));
            while (self.regs().cmd.read() & (1 << 18)) != 0 {}
            for i in 0..((chunk_len + 3) / 4) {
                let word = self.regs().w[i].read();
                for b in 0..4 {
                    let idx = offset + i * 4 + b;
                    if idx - offset < chunk_len {
                        rx_buffer[idx] = (word >> (b * 8)) as u8;
                    }
                }
            }
            offset += chunk_len;
        }
        Ok(())
    }

    fn write(&mut self, data: &[u8]) -> HalResult<()> {
        let mut offset = 0;
        while offset < data.len() {
            let chunk_len = (data.len() - offset).min(64);
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(bit_len - 1);
            self.regs().miso_dlen.write(0);
            for i in 0..((chunk_len + 3) / 4) {
                let mut word = 0u32;
                for b in 0..4 {
                    let idx = offset + i * 4 + b;
                    if idx - offset < chunk_len {
                        word |= (data[idx] as u32) << (b * 8);
                    }
                }
                self.regs().w[i].write(word);
            }
            self.regs().cmd.modify(|v| v | (1 << 18));
            while (self.regs().cmd.read() & (1 << 18)) != 0 {}
            offset += chunk_len;
        }
        Ok(())
    }

    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()> {
        let mut offset = 0;
        while offset < buffer.len() {
            let chunk_len = (buffer.len() - offset).min(64);
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(0);
            self.regs().miso_dlen.write(bit_len - 1);
            self.regs().cmd.modify(|v| v | (1 << 18));
            while (self.regs().cmd.read() & (1 << 18)) != 0 {}
            for i in 0..((chunk_len + 3) / 4) {
                let word = self.regs().w[i].read();
                for b in 0..4 {
                    let idx = offset + i * 4 + b;
                    if idx - offset < chunk_len {
                        buffer[idx] = (word >> (b * 8)) as u8;
                    }
                }
            }
            offset += chunk_len;
        }
        Ok(())
    }
}
