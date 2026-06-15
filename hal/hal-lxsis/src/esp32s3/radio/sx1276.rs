//! SX1276 LoRa transceiver driver.
//!
//! Communicates via SPI (address byte + data byte(s) per transaction).
//! Register write: `[addr | 0x80, value]`; register read: `[addr & 0x7F, 0x00]`.

use hal::error::{HalError, HalResult};
use hal::gpio::{GpioPin, Level, PinMode};
use hal::lora::{Bandwidth, CodingRate, LoRaTxConfig, RfDriver};
use hal::spi::SpiMaster;
use crate::esp32s3::Esp32S3Pin;

// Register map
const REG_FIFO:           u8 = 0x00;
const REG_OP_MODE:        u8 = 0x01;
const REG_FR_MSB:         u8 = 0x06;
const REG_FR_MID:         u8 = 0x07;
const REG_FR_LSB:         u8 = 0x08;
const REG_PA_CONFIG:      u8 = 0x09;
const REG_FIFO_ADDR_PTR:  u8 = 0x0D;
const REG_FIFO_TX_BASE:   u8 = 0x0E;
const REG_MODEM_CFG1:     u8 = 0x1D;
const REG_MODEM_CFG2:     u8 = 0x1E;
const REG_PREAMBLE_MSB:   u8 = 0x20;
const REG_PREAMBLE_LSB:   u8 = 0x21;
const REG_PAYLOAD_LENGTH: u8 = 0x22;
const REG_MODEM_CFG3:     u8 = 0x26;
const REG_DIO_MAPPING1:   u8 = 0x40;
const REG_VERSION:        u8 = 0x42;

const LORA_MODE:          u8 = 0x80;
const MODE_SLEEP:         u8 = 0x00;
const MODE_STDBY:         u8 = 0x01;
const MODE_TX:            u8 = 0x03;
const PA_BOOST:           u8 = 0x80;
const EXPECTED_VERSION:   u8 = 0x12;

/// SX1276 radio driver.
///
/// Generic over any [`SpiMaster`] implementation so it can run on real
/// hardware and under simulation (Renode) without modification.
pub struct Sx1276<SPI> {
    spi:   SPI,
    reset: Esp32S3Pin,
}

impl<SPI: SpiMaster> Sx1276<SPI> {
    pub fn new(spi: SPI, reset: Esp32S3Pin) -> Self {
        Self { spi, reset }
    }

    fn write_reg(&mut self, addr: u8, value: u8) -> HalResult<()> {
        self.spi.write(&[addr | 0x80, value])
    }

    fn read_reg(&mut self, addr: u8) -> HalResult<u8> {
        let mut buf = [0u8; 2];
        self.spi.transfer(&[addr & 0x7F, 0x00], &mut buf)?;
        Ok(buf[1])
    }

    /// Write bytes to the FIFO using a single SPI transaction (max 255 B).
    fn write_fifo(&mut self, data: &[u8]) -> HalResult<()> {
        let n = data.len().min(255);
        let mut buf = [0u8; 256];
        buf[0] = REG_FIFO | 0x80;
        buf[1..=n].copy_from_slice(&data[..n]);
        self.spi.write(&buf[..=n])
    }

    fn set_frequency(&mut self, freq_hz: u32) -> HalResult<()> {
        // frf = freq_hz / (32 MHz / 2^19) = freq_hz * 524288 / 32_000_000
        let frf = ((freq_hz as u64) << 19) / 32_000_000;
        self.write_reg(REG_FR_MSB, (frf >> 16) as u8)?;
        self.write_reg(REG_FR_MID, (frf >> 8) as u8)?;
        self.write_reg(REG_FR_LSB, frf as u8)
    }

    fn hard_reset(&mut self) -> HalResult<()> {
        self.reset.set_mode(PinMode::Output).map_err(|_| HalError::HardwareError)?;
        self.reset.write(Level::Low).map_err(|_| HalError::HardwareError)?;
        for _ in 0..100_000u32 { core::hint::spin_loop(); }  // ≥100 μs
        self.reset.write(Level::High).map_err(|_| HalError::HardwareError)?;
        for _ in 0..500_000u32 { core::hint::spin_loop(); }  // ≥5 ms
        Ok(())
    }
}

impl<SPI: SpiMaster> RfDriver for Sx1276<SPI> {
    fn chip_name(&self) -> &'static str { "SX1276" }

    fn init(&mut self) -> HalResult<()> {
        self.hard_reset()?;
        let ver = self.read_reg(REG_VERSION)?;
        if ver != EXPECTED_VERSION {
            return Err(HalError::HardwareError);
        }
        // Switch to SLEEP then enable LoRa mode bit
        self.write_reg(REG_OP_MODE, MODE_SLEEP)?;
        self.write_reg(REG_OP_MODE, LORA_MODE | MODE_SLEEP)?;
        // Fixed TX FIFO base at 0x00
        self.write_reg(REG_FIFO_TX_BASE, 0x00)?;
        // DIO0 → TxDone
        self.write_reg(REG_DIO_MAPPING1, 0x40)?;
        Ok(())
    }

    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> HalResult<()> {
        self.write_reg(REG_OP_MODE, LORA_MODE | MODE_STDBY)?;
        self.set_frequency(cfg.channel.frequency_hz)?;

        // RegModemConfig1: BW[7:4] | CR[3:1] | ImplicitHeader=0
        let bw_bits: u8 = match cfg.modulation.bw {
            Bandwidth::Bw125 => 0x70,
            Bandwidth::Bw250 => 0x80,
            Bandwidth::Bw500 => 0x90,
        };
        let cr_bits: u8 = match cfg.modulation.cr {
            CodingRate::Cr45 => 0x02,
            CodingRate::Cr46 => 0x04,
            CodingRate::Cr47 => 0x06,
            CodingRate::Cr48 => 0x08,
        };
        self.write_reg(REG_MODEM_CFG1, bw_bits | cr_bits)?;

        // RegModemConfig2: SF[7:4] | TxContinuous=0 | RxCRC=1
        let sf_bits: u8 = (cfg.modulation.sf as u8) << 4;
        self.write_reg(REG_MODEM_CFG2, sf_bits | 0x04)?;

        // RegModemConfig3: LNA auto gain | LowDataRateOptimize for SF11/SF12
        let ldo: u8 = if cfg.modulation.sf as u8 >= 11 { 0x08 } else { 0x00 };
        self.write_reg(REG_MODEM_CFG3, 0x04 | ldo)?;

        // Preamble length
        self.write_reg(REG_PREAMBLE_MSB, (cfg.modulation.preamble >> 8) as u8)?;
        self.write_reg(REG_PREAMBLE_LSB, cfg.modulation.preamble as u8)?;

        // PA config — PA_BOOST pin, power = 2..17 dBm
        let pwr = cfg.tx_power_dbm.max(2).min(17) as u8 - 2;
        self.write_reg(REG_PA_CONFIG, PA_BOOST | pwr)?;

        // Load FIFO and set payload length
        self.write_reg(REG_FIFO_ADDR_PTR, 0x00)?;
        self.write_fifo(payload)?;
        self.write_reg(REG_PAYLOAD_LENGTH, payload.len().min(255) as u8)?;

        // Trigger TX (returns immediately; TxDone interrupt fires later)
        self.write_reg(REG_OP_MODE, LORA_MODE | MODE_TX)?;

        Ok(())
    }
}
