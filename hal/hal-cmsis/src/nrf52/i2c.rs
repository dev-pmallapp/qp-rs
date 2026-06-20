//! nRF52840 TWI (I2C) driver
//!
//! Implements [`embedded_hal::i2c::I2c`] for the nRF52840 TWIM peripheral.
//! The TWIM uses EasyDMA and the task/event model.

use hal::error::HalError;
use hal::mmio::RW;

/// nRF52840 TWIM register block (nRF52840 PS §34)
#[repr(C)]
pub struct TwimRegs {
    pub tasks_startrx:  RW<u32>,       // 0x000
    _pad0:              [u32; 1],
    pub tasks_starttx:  RW<u32>,       // 0x008
    _pad1:              [u32; 2],
    pub tasks_stop:     RW<u32>,       // 0x014
    _pad2:              [u32; 1],
    pub tasks_suspend:  RW<u32>,       // 0x01C
    pub tasks_resume:   RW<u32>,       // 0x020
    _pad3:              [u32; 56],
    pub events_stopped: RW<u32>,       // 0x104
    _pad4:              [u32; 1],
    pub events_error:   RW<u32>,       // 0x10C
    _pad5:              [u32; 8],
    pub events_rxstarted: RW<u32>,     // 0x12C
    pub events_txstarted: RW<u32>,     // 0x130
    _pad6:              [u32; 2],
    pub events_lastrx:  RW<u32>,       // 0x13C
    pub events_lasttx:  RW<u32>,       // 0x140
    _pad7:              [u32; 175],
    pub shorts:         RW<u32>,       // 0x200
    _pad8:              [u32; 56],
    pub intenset:       RW<u32>,       // 0x304
    pub intenclr:       RW<u32>,       // 0x308
    _pad9:              [u32; 110],
    pub errorsrc:       RW<u32>,       // 0x4C4
    _pad10:             [u32; 14],
    pub enable:         RW<u32>,       // 0x500
    _pad11:             [u32; 1],
    pub psel_scl:       RW<u32>,       // 0x508
    pub psel_sda:       RW<u32>,       // 0x50C
    _pad12:             [u32; 5],
    pub frequency:      RW<u32>,       // 0x524
    _pad13:             [u32; 3],
    pub rxd: TwimDmaBlock,          // 0x534
    pub txd: TwimDmaBlock,          // 0x544
    _pad14:             [u32; 13],
    pub address:        RW<u32>,       // 0x588
}

#[repr(C)]
pub struct TwimDmaBlock {
    pub ptr:    RW<u32>,
    pub maxcnt: RW<u32>,
    pub amount: RW<u32>,
    pub list:   RW<u32>,
}

const TWIM_ENABLE: u32 = 6;

/// nRF52840 TWIM master implementation.
pub struct Nrf52I2c {
    regs: *const TwimRegs,
}

unsafe impl Send for Nrf52I2c {}
unsafe impl Sync for Nrf52I2c {}

impl Nrf52I2c {
    /// Create a new TWIM handle.
    ///
    /// # Safety
    /// The caller must guarantee exclusive ownership of this TWIM peripheral.
    pub unsafe fn new(regs: *const TwimRegs) -> Self {
        Self { regs }
    }

    fn regs(&self) -> &TwimRegs {
        unsafe { &*self.regs }
    }

    /// Enable TWIM and set bus frequency.
    /// `freq`: 100_000 → K100, 250_000 → K250, 400_000 → K400
    pub fn configure(&mut self, freq: u32) {
        self.regs().enable.write(0); // disable first
        let freq_reg = match freq {
            f if f <= 100_000  => 0x0198_0000u32, // K100
            f if f <= 250_000  => 0x0400_0000,     // K250
            _                  => 0x0640_0000,     // K400
        };
        self.regs().frequency.write(freq_reg);
        self.regs().enable.write(TWIM_ENABLE);
    }

    fn do_write(&mut self, addr: u8, data: &[u8]) -> Result<(), HalError> {
        self.regs().address.write(addr as u32);
        self.regs().txd.ptr.write(data.as_ptr() as u32);
        self.regs().txd.maxcnt.write(data.len() as u32);
        self.regs().events_stopped.write(0);
        self.regs().events_error.write(0);
        self.regs().tasks_starttx.write(1);
        self.regs().tasks_stop.write(1);
        while self.regs().events_stopped.read() == 0 {}
        if self.regs().errorsrc.read() != 0 {
            return Err(HalError::HardwareError);
        }
        Ok(())
    }

    fn do_read(&mut self, addr: u8, buf: &mut [u8]) -> Result<(), HalError> {
        self.regs().address.write(addr as u32);
        self.regs().rxd.ptr.write(buf.as_mut_ptr() as u32);
        self.regs().rxd.maxcnt.write(buf.len() as u32);
        self.regs().events_stopped.write(0);
        self.regs().events_error.write(0);
        self.regs().tasks_startrx.write(1);
        self.regs().tasks_stop.write(1);
        while self.regs().events_stopped.read() == 0 {}
        if self.regs().errorsrc.read() != 0 {
            return Err(HalError::HardwareError);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// embedded-hal 1.0 I2c impl (7-bit addresses)
// ---------------------------------------------------------------------------
impl embedded_hal::i2c::ErrorType for Nrf52I2c {
    type Error = HalError;
}

impl embedded_hal::i2c::I2c<embedded_hal::i2c::SevenBitAddress> for Nrf52I2c {
    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.do_write(address, bytes)
    }

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<(), Self::Error> {
        self.do_read(address, buffer)
    }

    fn write_read(
        &mut self,
        address: u8,
        bytes: &[u8],
        buffer: &mut [u8],
    ) -> Result<(), Self::Error> {
        self.do_write(address, bytes)?;
        self.do_read(address, buffer)
    }

    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        for op in operations.iter_mut() {
            match op {
                embedded_hal::i2c::Operation::Write(bytes) => self.do_write(address, bytes)?,
                embedded_hal::i2c::Operation::Read(buf) => self.do_read(address, buf)?,
            }
        }
        Ok(())
    }
}
