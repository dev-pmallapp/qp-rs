//! ESP32-C3 SPI driver

use hal::spi::{SpiMaster, SpiConfig, SpiMode, BitOrder};
use hal::error::HalResult;
use super::regs::SpiRegs;

/// ESP32-C3 SPI Master
pub struct Esp32C3Spi {
    regs: *const SpiRegs,
}

unsafe impl Send for Esp32C3Spi {}
unsafe impl Sync for Esp32C3Spi {}

impl Esp32C3Spi {
    /// Create a new Esp32C3Spi handle
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

impl SpiMaster for Esp32C3Spi {
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()> {
        // SPI clock divider calculation from APB clock (80 MHz)
        let div = (80_000_000 / config.frequency).max(1);
        let clock_val = if div <= 1 {
            1 << 31 // CLK_EQU_SYSCLK
        } else {
            let pre = 0u32; // no prescaler
            let cnt_n = (div - 1) & 0x3F;
            let cnt_h = ((div / 2) - 1) & 0x3F;
            let cnt_l = cnt_n;
            cnt_l | (cnt_h << 6) | (cnt_n << 12) | (pre << 18)
        };
        self.regs().clock.write(clock_val);

        // Configure mode (CPOL/CPHA) and bit order
        let mut ctrl2 = 0;
        let mut user = (1 << 27) | (1 << 28); // USR_MOSI | USR_MISO

        match config.mode {
            SpiMode::Mode0 => {}
            SpiMode::Mode1 => {
                user |= 1 << 29; // CK_OUT_EDGE
            }
            SpiMode::Mode2 => {
                ctrl2 |= 1 << 24; // CK_OUT_LOW_MODE
                user |= 1 << 29; // CK_OUT_EDGE
            }
            SpiMode::Mode3 => {
                ctrl2 |= 1 << 24; // CK_OUT_LOW_MODE
            }
        }

        if config.bit_order == BitOrder::LsbFirst {
            user |= (1 << 16) | (1 << 17); // RD_BIT_ORDER | WR_BIT_ORDER
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

            // Write length in bits
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(bit_len - 1);
            self.regs().miso_dlen.write(bit_len - 1);

            // Load Tx data
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

            // Start user transfer (set USR, bit 18 of cmd)
            self.regs().cmd.modify(|v| v | (1 << 18));

            // Wait until transfer is complete (USR bit is cleared)
            while (self.regs().cmd.read() & (1 << 18)) != 0 {}

            // Read Rx data
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
        let total_len = data.len();
        let mut offset = 0;

        while offset < total_len {
            let chunk_len = (total_len - offset).min(64);
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(bit_len - 1);
            self.regs().miso_dlen.write(0);

            // Load Tx data
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
        let total_len = buffer.len();
        let mut offset = 0;

        while offset < total_len {
            let chunk_len = (total_len - offset).min(64);
            let bit_len = (chunk_len * 8) as u32;
            self.regs().mosi_dlen.write(0);
            self.regs().miso_dlen.write(bit_len - 1);

            self.regs().cmd.modify(|v| v | (1 << 18));
            while (self.regs().cmd.read() & (1 << 18)) != 0 {}

            // Read Rx data
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
