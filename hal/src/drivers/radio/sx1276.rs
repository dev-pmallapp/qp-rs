//! SX1276 LoRa transceiver driver.
//!
//! Generic driver operating over embedded-hal 1.0 `SpiBus` and `OutputPin`
//! traits. Suitable for any host microcontroller architecture.

use crate::error::{HalError, HalResult};
use crate::lora::{Bandwidth, CodingRate, LoRaTxConfig, RfDriver};
use crate::rf::{PhyEvent, RadioMode, RadioParams, RfPhy, RfRxConfig, RfTxConfig, RxMetadata};
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiBus;

// Register map
const REG_FIFO:                 u8 = 0x00;
const REG_OP_MODE:              u8 = 0x01;
const REG_FR_MSB:               u8 = 0x06;
const REG_FR_MID:               u8 = 0x07;
const REG_FR_LSB:               u8 = 0x08;
const REG_PA_CONFIG:            u8 = 0x09;
const REG_FIFO_ADDR_PTR:        u8 = 0x0D;
const REG_FIFO_TX_BASE:         u8 = 0x0E;
const REG_FIFO_RX_BASE:         u8 = 0x0F;
const REG_FIFO_RX_CURRENT_ADDR: u8 = 0x10;
const REG_IRQ_FLAGS:            u8 = 0x12;
const REG_RX_NB_BYTES:          u8 = 0x13;
const REG_PKT_SNR_VALUE:        u8 = 0x19;
const REG_PKT_RSSI_VALUE:       u8 = 0x1A;
const REG_RSSI_VALUE:           u8 = 0x1B;
const REG_MODEM_CFG1:           u8 = 0x1D;
const REG_MODEM_CFG2:           u8 = 0x1E;
const REG_PREAMBLE_MSB:         u8 = 0x20;
const REG_PREAMBLE_LSB:         u8 = 0x21;
const REG_PAYLOAD_LENGTH:       u8 = 0x22;
const REG_MODEM_CFG3:           u8 = 0x26;
const REG_DIO_MAPPING1:         u8 = 0x40;
const REG_VERSION:              u8 = 0x42;

// OpMode constants
const LORA_MODE:          u8 = 0x80;
const MODE_SLEEP:         u8 = 0x00;
const MODE_STDBY:         u8 = 0x01;
const MODE_TX:            u8 = 0x03;
const MODE_RX_CONTINUOUS: u8 = 0x05;
const MODE_CAD:           u8 = 0x07;

const PA_BOOST:           u8 = 0x80;
const EXPECTED_VERSION:   u8 = 0x12;

/// SX1276 Radio Transceiver Driver.
pub struct Sx1276<SPI, PIN> {
    spi: SPI,
    reset: PIN,
}

impl<SPI: SpiBus, PIN: OutputPin> Sx1276<SPI, PIN> {
    /// Create a new driver instance.
    pub fn new(spi: SPI, reset: PIN) -> Self {
        Self { spi, reset }
    }

    /// Write a transceiver register.
    fn write_reg(&mut self, addr: u8, value: u8) -> HalResult<()> {
        self.spi.write(&[addr | 0x80, value]).map_err(|_| HalError::HardwareError)
    }

    /// Read a transceiver register.
    fn read_reg(&mut self, addr: u8) -> HalResult<u8> {
        let mut buf = [0u8; 2];
        self.spi
            .transfer(&mut buf, &[addr & 0x7F, 0x00])
            .map_err(|_| HalError::HardwareError)?;
        Ok(buf[1])
    }

    /// Write bytes to the FIFO buffer.
    fn write_fifo(&mut self, data: &[u8]) -> HalResult<()> {
        let n = data.len().min(255);
        let mut buf = [0u8; 256];
        buf[0] = REG_FIFO | 0x80;
        buf[1..=n].copy_from_slice(&data[..n]);
        self.spi.write(&buf[..=n]).map_err(|_| HalError::HardwareError)
    }

    /// Encode and write carrier frequency registers.
    fn set_frequency(&mut self, freq_hz: u32) -> HalResult<()> {
        let frf = ((freq_hz as u64) << 19) / 32_000_000;
        self.write_reg(REG_FR_MSB, (frf >> 16) as u8)?;
        self.write_reg(REG_FR_MID, (frf >> 8) as u8)?;
        self.write_reg(REG_FR_LSB, frf as u8)
    }

    /// Toggle hardware reset pin.
    fn hard_reset(&mut self) -> HalResult<()> {
        self.reset.set_low().map_err(|_| HalError::HardwareError)?;
        for _ in 0..100_000u32 { core::hint::spin_loop(); }  // ≥100 μs
        self.reset.set_high().map_err(|_| HalError::HardwareError)?;
        for _ in 0..500_000u32 { core::hint::spin_loop(); }  // ≥5 ms
        Ok(())
    }
}

impl<SPI: SpiBus + Send, PIN: OutputPin + Send> RfPhy for Sx1276<SPI, PIN> {
    fn init(&mut self) -> HalResult<()> {
        self.hard_reset()?;
        let ver = self.read_reg(REG_VERSION)?;
        if ver != EXPECTED_VERSION {
            return Err(HalError::HardwareError);
        }
        // Switch to SLEEP mode to enable LoRa mode switch
        self.write_reg(REG_OP_MODE, MODE_SLEEP)?;
        self.write_reg(REG_OP_MODE, LORA_MODE | MODE_SLEEP)?;
        // Fixed TX base at 0x00, RX base at 0x00 (or 0x80)
        self.write_reg(REG_FIFO_TX_BASE, 0x00)?;
        self.write_reg(REG_FIFO_RX_BASE, 0x00)?;
        // DIO0 mappings: 0x40 maps DIO0 to TxDone in TX mode, and RxDone in RX mode.
        self.write_reg(REG_DIO_MAPPING1, 0x40)?;
        Ok(())
    }

    fn set_mode(&mut self, mode: RadioMode) -> HalResult<()> {
        match mode {
            RadioMode::Sleep => {
                self.write_reg(REG_OP_MODE, LORA_MODE | MODE_SLEEP)
            }
            RadioMode::Standby => {
                self.write_reg(REG_OP_MODE, LORA_MODE | MODE_STDBY)
            }
            RadioMode::Rx { .. } => {
                self.write_reg(REG_OP_MODE, LORA_MODE | MODE_RX_CONTINUOUS)
            }
            RadioMode::Tx => {
                self.write_reg(REG_OP_MODE, LORA_MODE | MODE_TX)
            }
            RadioMode::Cad => {
                self.write_reg(REG_OP_MODE, LORA_MODE | MODE_CAD)
            }
        }
    }

    fn configure_tx(&mut self, cfg: &RfTxConfig) -> HalResult<()> {
        self.set_mode(RadioMode::Standby)?;
        self.set_frequency(cfg.frequency_hz)?;

        match cfg.params {
            RadioParams::LoRa(ref lora) => {
                // Config 1: BW | CR | ImplicitHeader=0
                let bw_bits: u8 = match lora.bw {
                    Bandwidth::Bw125 => 0x70,
                    Bandwidth::Bw250 => 0x80,
                    Bandwidth::Bw500 => 0x90,
                };
                let cr_bits: u8 = match lora.cr {
                    CodingRate::Cr45 => 0x02,
                    CodingRate::Cr46 => 0x04,
                    CodingRate::Cr47 => 0x06,
                    CodingRate::Cr48 => 0x08,
                };
                self.write_reg(REG_MODEM_CFG1, bw_bits | cr_bits)?;

                // Config 2: SF | TxContinuous=0 | RxPayloadCrcOn=1 (0x04)
                let sf_bits: u8 = (lora.sf as u8) << 4;
                self.write_reg(REG_MODEM_CFG2, sf_bits | 0x04)?;

                // Config 3: LNA auto gain (0x04) | LowDataRateOptimize (0x08) for SF11/SF12
                let ldo: u8 = if lora.sf as u8 >= 11 { 0x08 } else { 0x00 };
                self.write_reg(REG_MODEM_CFG3, 0x04 | ldo)?;

                // Preamble length
                self.write_reg(REG_PREAMBLE_MSB, (lora.preamble >> 8) as u8)?;
                self.write_reg(REG_PREAMBLE_LSB, lora.preamble as u8)?;
            }
        }

        // PA config: PA_BOOST pin, power config (2..17 dBm)
        let pwr = cfg.tx_power_dbm.max(2).min(17) as u8 - 2;
        self.write_reg(REG_PA_CONFIG, PA_BOOST | pwr)?;
        Ok(())
    }

    fn configure_rx(&mut self, cfg: &RfRxConfig) -> HalResult<()> {
        self.set_mode(RadioMode::Standby)?;
        self.set_frequency(cfg.frequency_hz)?;

        match cfg.params {
            RadioParams::LoRa(ref lora) => {
                let bw_bits: u8 = match lora.bw {
                    Bandwidth::Bw125 => 0x70,
                    Bandwidth::Bw250 => 0x80,
                    Bandwidth::Bw500 => 0x90,
                };
                let cr_bits: u8 = match lora.cr {
                    CodingRate::Cr45 => 0x02,
                    CodingRate::Cr46 => 0x04,
                    CodingRate::Cr47 => 0x06,
                    CodingRate::Cr48 => 0x08,
                };
                self.write_reg(REG_MODEM_CFG1, bw_bits | cr_bits)?;

                let sf_bits: u8 = (lora.sf as u8) << 4;
                self.write_reg(REG_MODEM_CFG2, sf_bits | 0x04)?;

                let ldo: u8 = if lora.sf as u8 >= 11 { 0x08 } else { 0x00 };
                self.write_reg(REG_MODEM_CFG3, 0x04 | ldo)?;

                self.write_reg(REG_PREAMBLE_MSB, (lora.preamble >> 8) as u8)?;
                self.write_reg(REG_PREAMBLE_LSB, lora.preamble as u8)?;
            }
        }
        Ok(())
    }

    fn transmit(&mut self, payload: &[u8]) -> HalResult<()> {
        // Prepare FIFO pointers
        self.write_reg(REG_FIFO_ADDR_PTR, 0x00)?;
        self.write_fifo(payload)?;
        self.write_reg(REG_PAYLOAD_LENGTH, payload.len().min(255) as u8)?;

        // Set mode to TX
        self.set_mode(RadioMode::Tx)
    }

    fn read_rx(&mut self, buf: &mut [u8], meta: &RxMetadata) -> HalResult<()> {
        let current_addr = self.read_reg(REG_FIFO_RX_CURRENT_ADDR)?;
        self.write_reg(REG_FIFO_ADDR_PTR, current_addr)?;

        let len = (meta.pkt_len as usize).min(buf.len());
        for i in 0..len {
            buf[i] = self.read_reg(REG_FIFO)?;
        }
        Ok(())
    }

    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>> {
        let irq_flags = self.read_reg(REG_IRQ_FLAGS)?;
        if irq_flags == 0 {
            return Ok(None);
        }
        // Clear flags by writing back
        self.write_reg(REG_IRQ_FLAGS, irq_flags)?;

        // Check flags: RxTimeout(bit 7), RxDone(bit 6), PayloadCrcError(bit 5), TxDone(bit 3), CadDone(bit 2)
        if irq_flags & 0x20 != 0 {
            Ok(Some(PhyEvent::CrcError))
        } else if irq_flags & 0x80 != 0 {
            Ok(Some(PhyEvent::RxTimeout))
        } else if irq_flags & 0x40 != 0 {
            // Read RX metadata
            let pkt_len = self.read_reg(REG_RX_NB_BYTES)?;
            let snr_reg = self.read_reg(REG_PKT_SNR_VALUE)?;
            let snr = (snr_reg as i8) as i16 * 10 / 4; // tenths of dB

            let rssi_reg = self.read_reg(REG_PKT_RSSI_VALUE)? as i16;
            let rssi = if snr < 0 {
                -157 + rssi_reg + snr / 10
            } else {
                -157 + rssi_reg
            };

            Ok(Some(PhyEvent::RxDone(RxMetadata {
                rssi_dbm: rssi,
                snr_db_x10: snr,
                timestamp: 0,
                pkt_len,
            })))
        } else if irq_flags & 0x08 != 0 {
            Ok(Some(PhyEvent::TxDone))
        } else if irq_flags & 0x04 != 0 {
            Ok(Some(PhyEvent::CadDone { channel_active: true }))
        } else {
            Ok(None)
        }
    }

    fn clear_irq(&mut self) -> HalResult<()> {
        self.write_reg(REG_IRQ_FLAGS, 0xFF)
    }

    fn rssi(&mut self) -> HalResult<i16> {
        let rssi_reg = self.read_reg(REG_RSSI_VALUE)?;
        Ok(-157 + rssi_reg as i16)
    }

    fn chip_name(&self) -> &'static str {
        "SX1276"
    }
}

impl<SPI: SpiBus + Send, PIN: OutputPin + Send> RfDriver for Sx1276<SPI, PIN> {
    fn init(&mut self) -> HalResult<()> {
        <Self as RfPhy>::init(self)
    }

    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> HalResult<()> {
        let tx_cfg = RfTxConfig {
            frequency_hz: cfg.channel.frequency_hz,
            tx_power_dbm: cfg.tx_power_dbm,
            params: RadioParams::LoRa(cfg.modulation.clone()),
        };
        <Self as RfPhy>::configure_tx(self, &tx_cfg)?;
        <Self as RfPhy>::transmit(self, payload)
    }

    fn chip_name(&self) -> &'static str {
        <Self as RfPhy>::chip_name(self)
    }
}
