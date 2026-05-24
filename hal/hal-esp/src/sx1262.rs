//! SX1262 LoRa transceiver driver.
//!
//! SX1262 uses a command-based SPI interface (opcode + parameters), unlike
//! the SX1276 register map.  A BUSY GPIO output indicates when the chip is
//! processing a command; poll it low before issuing the next one.

use hal::error::{HalError, HalResult};
use hal::gpio::{GpioPin, Level, PinMode};
use hal::lora::{Bandwidth, CodingRate, LoRaTxConfig, RfDriver, SpreadingFactor};
use hal::spi::SpiMaster;
use crate::gpio::EspGpioPin;

// SX1262 opcode commands
const CMD_SET_SLEEP:            u8 = 0x84;
const CMD_SET_STANDBY:          u8 = 0x80;
const CMD_SET_RF_FREQUENCY:     u8 = 0x86;
const CMD_SET_TX_PARAMS:        u8 = 0x8E;
const CMD_SET_MODULATION:       u8 = 0x8B;
const CMD_SET_PKT_PARAMS:       u8 = 0x8C;
const CMD_SET_BUF_BASE_ADDR:    u8 = 0x8F;
const CMD_WRITE_BUFFER:         u8 = 0x0E;
const CMD_SET_TX:               u8 = 0x83;
const CMD_GET_STATUS:           u8 = 0xC0;

// Modulation parameter encodings
const SF_OFFSET: u8 = 0; // SF7 = 0x07, SF8 = 0x08, ..., SF12 = 0x0C
const BW_125K:   u8 = 0x04;
const BW_250K:   u8 = 0x05;
const BW_500K:   u8 = 0x06;
const CR_45:     u8 = 0x01;
const CR_46:     u8 = 0x02;
const CR_47:     u8 = 0x03;
const CR_48:     u8 = 0x04;

/// SX1262 radio driver.
///
/// Generic over any [`SpiMaster`] implementation.
/// `busy_pin` is optional: omit it in simulation where the BUSY line is
/// always low (Renode peripheral responds immediately).
pub struct Sx1262<SPI> {
    spi:   SPI,
    reset: EspGpioPin,
    busy:  Option<EspGpioPin>,
}

impl<SPI: SpiMaster> Sx1262<SPI> {
    /// Create a driver without a BUSY pin (suitable for simulation).
    pub fn new(spi: SPI, reset: EspGpioPin) -> Self {
        Self { spi, reset, busy: None }
    }

    /// Create a driver with a BUSY pin (required on real hardware).
    pub fn with_busy(spi: SPI, reset: EspGpioPin, busy: EspGpioPin) -> Self {
        Self { spi, reset, busy: Some(busy) }
    }

    /// Poll the BUSY pin low before sending the next command.
    /// Without a BUSY pin, returns immediately (simulation mode).
    fn wait_busy(&mut self) -> HalResult<()> {
        let Some(ref mut busy) = self.busy else { return Ok(()); };
        busy.set_mode(PinMode::Input).map_err(|_| HalError::HardwareError)?;
        for _ in 0..200_000u32 {
            match busy.read() {
                Ok(Level::Low)  => return Ok(()),
                Ok(Level::High) => core::hint::spin_loop(),
                Err(_)          => return Err(HalError::HardwareError),
            }
        }
        Err(HalError::Timeout)
    }

    /// Issue a command without expecting a response payload.
    fn write_cmd(&mut self, cmd: u8, params: &[u8]) -> HalResult<()> {
        self.wait_busy()?;
        // Stack buffer: 1-byte opcode + up to 7 parameter bytes
        let n = params.len().min(7);
        let mut buf = [0u8; 8];
        buf[0] = cmd;
        buf[1..=n].copy_from_slice(&params[..n]);
        self.spi.write(&buf[..=n])
    }

    /// WriteBuffer command: opcode + offset + up to 255 data bytes.
    fn write_buffer_cmd(&mut self, offset: u8, data: &[u8]) -> HalResult<()> {
        self.wait_busy()?;
        let n = data.len().min(255);
        let mut buf = [0u8; 257];
        buf[0] = CMD_WRITE_BUFFER;
        buf[1] = offset;
        buf[2..2 + n].copy_from_slice(&data[..n]);
        self.spi.write(&buf[..2 + n])
    }

    fn hard_reset(&mut self) -> HalResult<()> {
        self.reset.set_mode(PinMode::Output).map_err(|_| HalError::HardwareError)?;
        self.reset.write(Level::Low).map_err(|_| HalError::HardwareError)?;
        for _ in 0..100_000u32 { core::hint::spin_loop(); }  // ≥100 μs
        self.reset.write(Level::High).map_err(|_| HalError::HardwareError)?;
        for _ in 0..500_000u32 { core::hint::spin_loop(); }  // ≥5 ms
        Ok(())
    }

    fn set_rf_frequency(&mut self, freq_hz: u32) -> HalResult<()> {
        // rfFreq = (freq_hz / 32_000_000) * 2^25
        let rf_freq = ((freq_hz as u64 * (1u64 << 25)) / 32_000_000u64) as u32;
        self.write_cmd(CMD_SET_RF_FREQUENCY, &rf_freq.to_be_bytes())
    }
}

impl<SPI: SpiMaster> RfDriver for Sx1262<SPI> {
    fn chip_name(&self) -> &'static str { "SX1262" }

    fn init(&mut self) -> HalResult<()> {
        self.hard_reset()?;
        // Verify chip is responsive by reading status (0xC0 returns chip mode)
        self.wait_busy()?;
        let mut status_buf = [CMD_GET_STATUS, 0x00];
        let mut rx = [0u8; 2];
        self.spi.transfer(&status_buf, &mut rx)?;
        let chip_mode = (rx[1] >> 4) & 0x07;
        if chip_mode == 0 {
            return Err(HalError::HardwareError);  // chip not responding
        }

        // STDBY_RC mode
        self.write_cmd(CMD_SET_STANDBY, &[0x00])?;
        // TX base = 0x00, RX base = 0x80
        self.write_cmd(CMD_SET_BUF_BASE_ADDR, &[0x00, 0x80])?;
        Ok(())
    }

    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> HalResult<()> {
        // Standby
        self.write_cmd(CMD_SET_STANDBY, &[0x00])?;

        // RF frequency
        self.set_rf_frequency(cfg.channel.frequency_hz)?;

        // TX params: power (dBm, range -9..+22), ramp time 200μs
        let pwr = cfg.tx_power_dbm.max(-9).min(22) as u8;
        self.write_cmd(CMD_SET_TX_PARAMS, &[pwr, 0x04])?;

        // Modulation params: SF | BW | CR | LowDataRateOptimize
        let sf: u8 = cfg.modulation.sf as u8; // SF7=7 .. SF12=12
        let bw: u8 = match cfg.modulation.bw {
            Bandwidth::Bw125 => BW_125K,
            Bandwidth::Bw250 => BW_250K,
            Bandwidth::Bw500 => BW_500K,
        };
        let cr: u8 = match cfg.modulation.cr {
            CodingRate::Cr45 => CR_45,
            CodingRate::Cr46 => CR_46,
            CodingRate::Cr47 => CR_47,
            CodingRate::Cr48 => CR_48,
        };
        let ldo: u8 = if cfg.modulation.sf as u8 >= 11 { 0x01 } else { 0x00 };
        self.write_cmd(CMD_SET_MODULATION, &[sf, bw, cr, ldo])?;

        // Packet params: preamble(2B) | headerType=variable | payloadLen | CRC=on | invertIQ=off
        let preamble = cfg.modulation.preamble;
        let plen = payload.len().min(255) as u8;
        let pkt_params = [
            (preamble >> 8) as u8,
            preamble as u8,
            0x00, // variable-length header
            plen,
            0x01, // CRC on
            0x00, // standard IQ
        ];
        self.write_cmd(CMD_SET_PKT_PARAMS, &pkt_params)?;

        // Write payload to TX buffer at offset 0
        self.write_buffer_cmd(0x00, payload)?;

        // SetTx with no timeout (single-shot)
        self.write_cmd(CMD_SET_TX, &[0x00, 0x00, 0x00])?;

        Ok(())
    }
}
