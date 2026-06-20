//! STM32F4 I2C driver
//!
//! Implements [`embedded_hal::i2c::I2c`] for the STM32F4 I2C peripheral.
//! Register offsets follow RM0090 rev 19 §27.

use hal::error::HalError;
use hal::mmio::RW;

/// STM32F4 I2C register block (RM0090 §27.6)
#[repr(C)]
pub struct I2cRegs {
    pub cr1:  RW<u32>,   // 0x00 Control register 1
    pub cr2:  RW<u32>,   // 0x04 Control register 2
    pub oar1: RW<u32>,   // 0x08 Own address register 1
    pub oar2: RW<u32>,   // 0x0C Own address register 2
    pub dr:   RW<u32>,   // 0x10 Data register
    pub sr1:  RW<u32>,   // 0x14 Status register 1
    pub sr2:  RW<u32>,   // 0x18 Status register 2
    pub ccr:  RW<u32>,   // 0x1C Clock control register
    pub trise:RW<u32>,   // 0x20 TRISE register
    pub fltr: RW<u32>,   // 0x24 FLTR register
}

// SR1 bit positions
const SR1_SB:    u32 = 1 << 0;  // Start bit generated
const SR1_ADDR:  u32 = 1 << 1;  // Address sent/matched
const SR1_TXE:   u32 = 1 << 7;  // Data register empty (TX)
const SR1_RXNE:  u32 = 1 << 6;  // Data register not empty (RX)
const SR1_BTF:   u32 = 1 << 2;  // Byte transfer finished
const SR1_AF:    u32 = 1 << 10; // Acknowledge failure
const SR1_ARLO:  u32 = 1 << 9;  // Arbitration lost
const SR1_BERR:  u32 = 1 << 8;  // Bus error

// CR1 bit positions
const CR1_PE:    u32 = 1 << 0;  // Peripheral enable
const CR1_START: u32 = 1 << 8;  // Start generation
const CR1_STOP:  u32 = 1 << 9;  // Stop generation
const CR1_ACK:   u32 = 1 << 10; // Acknowledge enable

/// STM32F4 I2C master implementation.
pub struct Stm32F4I2c {
    regs: *const I2cRegs,
    /// Peripheral clock in Hz (used for CCR calculation)
    pclk_hz: u32,
}

unsafe impl Send for Stm32F4I2c {}
unsafe impl Sync for Stm32F4I2c {}

impl Stm32F4I2c {
    /// Create a new I2C handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of this I2C peripheral.
    pub unsafe fn new(regs: *const I2cRegs, pclk_hz: u32) -> Self {
        Self { regs, pclk_hz }
    }

    fn regs(&self) -> &I2cRegs {
        unsafe { &*self.regs }
    }

    /// Configure the I2C clock for the given frequency in Hz.
    pub fn configure_clock(&mut self, freq_hz: u32) {
        // Disable peripheral during configuration
        self.regs().cr1.modify(|v| v & !CR1_PE);

        let freq_mhz = (self.pclk_hz / 1_000_000).max(2).min(50);
        self.regs().cr2.write(freq_mhz);

        // Standard mode (Sm): CCR = f_PCLK / (2 * f_SCL)
        // Fast mode (Fm): CCR = f_PCLK / (3 * f_SCL) for duty=0
        let (ccr_val, duty) = if freq_hz <= 100_000 {
            (self.pclk_hz / (2 * freq_hz), 0u32)
        } else {
            (self.pclk_hz / (3 * freq_hz), 1u32 << 15)
        };
        self.regs().ccr.write(ccr_val | duty);

        // TRISE = (f_PCLK_MHz + 1) for Sm; ((f_PCLK_MHz * 300 / 1000) + 1) for Fm
        let trise = if freq_hz <= 100_000 {
            freq_mhz + 1
        } else {
            (freq_mhz * 3 / 10) + 1
        };
        self.regs().trise.write(trise);

        // Re-enable with ACK
        self.regs().cr1.write(CR1_PE | CR1_ACK);
    }

    fn check_errors(&self) -> Result<(), HalError> {
        let sr1 = self.regs().sr1.read();
        if sr1 & SR1_AF   != 0 { return Err(HalError::HardwareError); }
        if sr1 & SR1_ARLO != 0 { return Err(HalError::Busy); }
        if sr1 & SR1_BERR != 0 { return Err(HalError::HardwareError); }
        Ok(())
    }

    fn start(&mut self) -> Result<(), HalError> {
        self.regs().cr1.modify(|v| v | CR1_START);
        while self.regs().sr1.read() & SR1_SB == 0 {
            self.check_errors()?;
        }
        Ok(())
    }

    fn send_addr(&mut self, addr: u8, read: bool) -> Result<(), HalError> {
        self.regs().dr.write(((addr as u32) << 1) | (read as u32));
        while self.regs().sr1.read() & SR1_ADDR == 0 {
            self.check_errors()?;
        }
        // Clear ADDR by reading SR1 then SR2
        let _ = self.regs().sr1.read();
        let _ = self.regs().sr2.read();
        Ok(())
    }

    fn stop(&mut self) {
        self.regs().cr1.modify(|v| v | CR1_STOP);
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 I2c impl (7-bit addresses)
// ---------------------------------------------------------------------------
impl embedded_hal::i2c::ErrorType for Stm32F4I2c {
    type Error = HalError;
}

impl embedded_hal::i2c::I2c<embedded_hal::i2c::SevenBitAddress> for Stm32F4I2c {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.start()?;
        self.send_addr(address, false)?;
        for &b in bytes {
            while self.regs().sr1.read() & SR1_TXE == 0 {
                self.check_errors()?;
            }
            self.regs().dr.write(b as u32);
        }
        while self.regs().sr1.read() & SR1_BTF == 0 {
            self.check_errors()?;
        }
        self.stop();
        Ok(())
    }

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.start()?;
        self.send_addr(address, true)?;
        let len = buffer.len();
        for (i, slot) in buffer.iter_mut().enumerate() {
            if i == len - 1 {
                // Disable ACK before reading last byte
                self.regs().cr1.modify(|v| v & !CR1_ACK);
                self.stop();
            }
            while self.regs().sr1.read() & SR1_RXNE == 0 {
                self.check_errors()?;
            }
            *slot = self.regs().dr.read() as u8;
        }
        // Re-enable ACK for next transaction
        self.regs().cr1.modify(|v| v | CR1_ACK);
        Ok(())
    }

    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.start()?;
        self.send_addr(address, false)?;
        for &b in bytes {
            while self.regs().sr1.read() & SR1_TXE == 0 {
                self.check_errors()?;
            }
            self.regs().dr.write(b as u32);
        }
        while self.regs().sr1.read() & SR1_BTF == 0 {
            self.check_errors()?;
        }
        // Repeated start
        self.start()?;
        self.send_addr(address, true)?;
        let len = buffer.len();
        for (i, slot) in buffer.iter_mut().enumerate() {
            if i == len - 1 {
                self.regs().cr1.modify(|v| v & !CR1_ACK);
                self.stop();
            }
            while self.regs().sr1.read() & SR1_RXNE == 0 {
                self.check_errors()?;
            }
            *slot = self.regs().dr.read() as u8;
        }
        self.regs().cr1.modify(|v| v | CR1_ACK);
        Ok(())
    }

    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        for (idx, op) in operations.iter_mut().enumerate() {
            let is_first = idx == 0;
            match op {
                embedded_hal::i2c::Operation::Write(bytes) => {
                    if is_first { self.start()?; self.send_addr(address, false)?; }
                    for &b in bytes.iter() {
                        while self.regs().sr1.read() & SR1_TXE == 0 { self.check_errors()?; }
                        self.regs().dr.write(b as u32);
                    }
                    while self.regs().sr1.read() & SR1_BTF == 0 { self.check_errors()?; }
                }
                embedded_hal::i2c::Operation::Read(buf) => {
                    // Always issue a (repeated) start before a read segment
                    self.start()?;
                    self.send_addr(address, true)?;
                    let len = buf.len();
                    for (i, slot) in buf.iter_mut().enumerate() {
                        if i == len - 1 {
                            self.regs().cr1.modify(|v| v & !CR1_ACK);
                        }
                        while self.regs().sr1.read() & SR1_RXNE == 0 { self.check_errors()?; }
                        *slot = self.regs().dr.read() as u8;
                    }
                    self.regs().cr1.modify(|v| v | CR1_ACK);
                }
            }
        }
        self.stop();
        Ok(())
    }
}
