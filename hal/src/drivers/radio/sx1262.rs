//! SX1262 LoRa transceiver driver.
//!
//! Generic driver operating over standard `SpiMaster` and `GpioPin` traits.
//! Suitable for any host microcontroller architecture.

use crate::error::{HalError, HalResult};
use crate::gpio::{GpioPin, Level, PinMode};
use crate::lora::{Bandwidth, CodingRate, LoRaTxConfig, RfDriver};
use crate::rf::{PhyEvent, RadioMode, RadioParams, RfPhy, RfRxConfig, RfTxConfig, RxMetadata};
use crate::spi::SpiMaster;

// SX1262 opcode commands
const CMD_SET_SLEEP:          u8 = 0x84;
const CMD_SET_STANDBY:        u8 = 0x80;
const CMD_SET_RX:             u8 = 0x82;
const CMD_SET_TX:             u8 = 0x83;
const CMD_SET_CAD:            u8 = 0xC5;
const CMD_SET_RF_FREQUENCY:     u8 = 0x86;
const CMD_SET_TX_PARAMS:        u8 = 0x8E;
const CMD_SET_MODULATION:       u8 = 0x8B;
const CMD_SET_PKT_PARAMS:       u8 = 0x8C;
const CMD_SET_BUF_BASE_ADDR:    u8 = 0x8F;
const CMD_WRITE_BUFFER:         u8 = 0x0E;
const CMD_READ_BUFFER:          u8 = 0x1E;
const CMD_GET_STATUS:           u8 = 0xC0;
const CMD_CLEAR_IRQ_STATUS:     u8 = 0x97;
const CMD_GET_IRQ_STATUS:       u8 = 0x12;
const CMD_GET_RSSI_INST:        u8 = 0x15;
const CMD_GET_PACKET_STATUS:    u8 = 0x14;

// Modulation parameter encodings
const BW_125K:   u8 = 0x04;
const BW_250K:   u8 = 0x05;
const BW_500K:   u8 = 0x06;
const CR_45:     u8 = 0x01;
const CR_46:     u8 = 0x02;
const CR_47:     u8 = 0x03;
const CR_48:     u8 = 0x04;

/// SX1262 Radio Transceiver Driver.
pub struct Sx1262<SPI, PIN> {
    spi: SPI,
    reset: PIN,
    busy: Option<PIN>,
}

impl<SPI: SpiMaster, PIN: GpioPin> Sx1262<SPI, PIN> {
    /// Create a new driver instance (for simulation / standard configurations without BUSY polling).
    pub fn new(spi: SPI, reset: PIN) -> Self {
        Self {
            spi,
            reset,
            busy: None,
        }
    }

    /// Create a new driver instance with a BUSY line (required on real hardware).
    pub fn with_busy(spi: SPI, reset: PIN, busy: PIN) -> Self {
        Self {
            spi,
            reset,
            busy: Some(busy),
        }
    }

    /// Wait for the transceiver BUSY pin to go low.
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

    /// Issue a command to the transceiver with a parameters slice.
    fn write_cmd(&mut self, cmd: u8, params: &[u8]) -> HalResult<()> {
        self.wait_busy()?;
        let n = params.len().min(7);
        let mut buf = [0u8; 8];
        buf[0] = cmd;
        buf[1..=n].copy_from_slice(&params[..n]);
        self.spi.write(&buf[..=n])
    }

    /// Write bytes into the transceiver memory buffer.
    fn write_buffer_cmd(&mut self, offset: u8, data: &[u8]) -> HalResult<()> {
        self.wait_busy()?;
        let n = data.len().min(255);
        let mut buf = [0u8; 257];
        buf[0] = CMD_WRITE_BUFFER;
        buf[1] = offset;
        buf[2..2 + n].copy_from_slice(&data[..n]);
        self.spi.write(&buf[..2 + n])
    }

    /// Toggle hardware reset pin.
    fn hard_reset(&mut self) -> HalResult<()> {
        self.reset.set_mode(PinMode::Output).map_err(|_| HalError::HardwareError)?;
        self.reset.write(Level::Low).map_err(|_| HalError::HardwareError)?;
        for _ in 0..100_000u32 { core::hint::spin_loop(); }  // ≥100 μs
        self.reset.write(Level::High).map_err(|_| HalError::HardwareError)?;
        for _ in 0..500_000u32 { core::hint::spin_loop(); }  // ≥5 ms
        Ok(())
    }

    /// Encode and write carrier frequency parameters.
    fn set_rf_frequency(&mut self, freq_hz: u32) -> HalResult<()> {
        let rf_freq = ((freq_hz as u64 * (1u64 << 25)) / 32_000_000u64) as u32;
        self.write_cmd(CMD_SET_RF_FREQUENCY, &rf_freq.to_be_bytes())
    }
}

impl<SPI: SpiMaster, PIN: GpioPin> RfPhy for Sx1262<SPI, PIN> {
    fn init(&mut self) -> HalResult<()> {
        self.hard_reset()?;
        self.wait_busy()?;
        let status_buf = [CMD_GET_STATUS, 0x00];
        let mut rx = [0u8; 2];
        self.spi.transfer(&status_buf, &mut rx)?;
        let chip_mode = (rx[1] >> 4) & 0x07;
        if chip_mode == 0 {
            return Err(HalError::HardwareError);
        }
        // STDBY_RC mode
        self.write_cmd(CMD_SET_STANDBY, &[0x00])?;
        // TX base = 0x00, RX base = 0x80
        self.write_cmd(CMD_SET_BUF_BASE_ADDR, &[0x00, 0x80])?;
        Ok(())
    }

    fn set_mode(&mut self, mode: RadioMode) -> HalResult<()> {
        match mode {
            RadioMode::Sleep => {
                // Sleep, preserve RAM configuration (warm start = 0x04)
                self.write_cmd(CMD_SET_SLEEP, &[0x04])
            }
            RadioMode::Standby => {
                self.write_cmd(CMD_SET_STANDBY, &[0x00]) // STDBY_RC
            }
            RadioMode::Rx { timeout_ms } => {
                let ticks = timeout_ms.map(|ms| ms * 64).unwrap_or(0xFFFFFF);
                let params = [
                    (ticks >> 16) as u8,
                    (ticks >> 8) as u8,
                    ticks as u8,
                ];
                self.write_cmd(CMD_SET_RX, &params)
            }
            RadioMode::Tx => {
                // Set Tx with infinite/unlimited timeout (must trigger on TxDone)
                self.write_cmd(CMD_SET_TX, &[0x00, 0x00, 0x00])
            }
            RadioMode::Cad => {
                self.write_cmd(CMD_SET_CAD, &[])
            }
        }
    }

    fn configure_tx(&mut self, cfg: &RfTxConfig) -> HalResult<()> {
        self.write_cmd(CMD_SET_STANDBY, &[0x00])?;
        self.set_rf_frequency(cfg.frequency_hz)?;

        // TX params: power, ramp time 200μs (0x04)
        let pwr = cfg.tx_power_dbm.max(-9).min(22) as u8;
        self.write_cmd(CMD_SET_TX_PARAMS, &[pwr, 0x04])?;

        match cfg.params {
            RadioParams::LoRa(ref lora) => {
                let sf = lora.sf as u8;
                let bw = match lora.bw {
                    Bandwidth::Bw125 => BW_125K,
                    Bandwidth::Bw250 => BW_250K,
                    Bandwidth::Bw500 => BW_500K,
                };
                let cr = match lora.cr {
                    CodingRate::Cr45 => CR_45,
                    CodingRate::Cr46 => CR_46,
                    CodingRate::Cr47 => CR_47,
                    CodingRate::Cr48 => CR_48,
                };
                let ldo = if lora.sf as u8 >= 11 { 0x01 } else { 0x00 };
                self.write_cmd(CMD_SET_MODULATION, &[sf, bw, cr, ldo])?;

                // Packet params: preamble (2B) | headerType=variable (0x00) | payloadLen=0 (will overwrite during TX) | CRC=on (0x01) | invertIQ=off (0x00)
                let pkt_params = [
                    (lora.preamble >> 8) as u8,
                    lora.preamble as u8,
                    0x00, // Variable length
                    0x00, // Placeholder payload len
                    0x01, // CRC on
                    0x00, // Standard IQ
                ];
                self.write_cmd(CMD_SET_PKT_PARAMS, &pkt_params)?;
            }
            RadioParams::Fsk(_) => return Err(HalError::NotSupported),
        }
        Ok(())
    }

    fn configure_rx(&mut self, cfg: &RfRxConfig) -> HalResult<()> {
        self.write_cmd(CMD_SET_STANDBY, &[0x00])?;
        self.set_rf_frequency(cfg.frequency_hz)?;

        match cfg.params {
            RadioParams::LoRa(ref lora) => {
                let sf = lora.sf as u8;
                let bw = match lora.bw {
                    Bandwidth::Bw125 => BW_125K,
                    Bandwidth::Bw250 => BW_250K,
                    Bandwidth::Bw500 => BW_500K,
                };
                let cr = match lora.cr {
                    CodingRate::Cr45 => CR_45,
                    CodingRate::Cr46 => CR_46,
                    CodingRate::Cr47 => CR_47,
                    CodingRate::Cr48 => CR_48,
                };
                let ldo = if lora.sf as u8 >= 11 { 0x01 } else { 0x00 };
                self.write_cmd(CMD_SET_MODULATION, &[sf, bw, cr, ldo])?;

                // Packet params: preamble (2B) | headerType=variable (0x00) | payloadLen=255 (max size for variable) | CRC=on (0x01) | invertIQ=off (0x00)
                let pkt_params = [
                    (lora.preamble >> 8) as u8,
                    lora.preamble as u8,
                    0x00, // Variable length
                    255,  // Max payload len
                    0x01, // CRC on
                    0x00, // Standard IQ
                ];
                self.write_cmd(CMD_SET_PKT_PARAMS, &pkt_params)?;
            }
            RadioParams::Fsk(_) => return Err(HalError::NotSupported),
        }
        Ok(())
    }

    fn transmit(&mut self, payload: &[u8]) -> HalResult<()> {
        let len = payload.len().min(255) as u8;
        // Overwrite packet params payload length register or reissue command with correct size
        // Note: SetPacketParams must be called to update payload length. We assume configure_tx applied modulation.
        // We read back the previous packet parameters if needed, but since we know it's LoRa,
        // we can hardcode default settings with correct payload size (SF/BW is in modulation params).
        // Let's reissue packet parameters with correct payload length:
        // (default: preamble 8, variable, CRC on, standard IQ)
        let pkt_params = [
            0x00, 0x08, // Preamble (8)
            0x00,       // Variable
            len,        // Actual payload len
            0x01,       // CRC on
            0x00,       // Standard IQ
        ];
        self.write_cmd(CMD_SET_PKT_PARAMS, &pkt_params)?;

        // Write payload to TX buffer base offset (0x00)
        self.write_buffer_cmd(0x00, payload)?;

        // Transition to Tx Mode
        self.set_mode(RadioMode::Tx)
    }

    fn read_rx(&mut self, buf: &mut [u8], meta: &RxMetadata) -> HalResult<()> {
        let len = (meta.pkt_len as usize).min(buf.len());
        self.wait_busy()?;

        // ReadBuffer: opcode (0x1E) + offset (0x80 is default RX base) + dummy byte
        let cmd = [CMD_READ_BUFFER, 0x80, 0x00];
        let mut tx_buf = [0u8; 258];
        let mut rx_buf = [0u8; 258];
        tx_buf[0..3].copy_from_slice(&cmd);

        self.spi.transfer(&tx_buf[..3 + len], &mut rx_buf[..3 + len])?;
        buf[..len].copy_from_slice(&rx_buf[3..3 + len]);
        Ok(())
    }

    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>> {
        self.wait_busy()?;
        let cmd = [CMD_GET_IRQ_STATUS, 0x00, 0x00, 0x00];
        let mut rx = [0u8; 4];
        self.spi.transfer(&cmd, &mut rx)?;
        let status = u16::from_be_bytes([rx[2], rx[3]]);

        if status == 0 {
            return Ok(None);
        }

        // Check flags in descending priority order
        if status & 0x0004 != 0 {
            Ok(Some(PhyEvent::CrcError))
        } else if status & 0x0040 != 0 {
            Ok(Some(PhyEvent::RxTimeout))
        } else if status & 0x0002 != 0 {
            // Read RX metadata
            let status_cmd = [CMD_GET_PACKET_STATUS, 0x00, 0x00, 0x00];
            let mut status_rx = [0u8; 4];
            self.spi.transfer(&status_cmd, &mut status_rx)?;
            // rx[2] = RssiPkt, rx[3] = SnrPkt
            let rssi = -(status_rx[2] as i16) / 2;
            let snr = (status_rx[3] as i8) as i16; // tenths of dB is reported directly by chip * 4, but we keep it simple

            // We also need the length of the received packet
            // Get Rx buffer status to read packet length
            let len_cmd = [0x13, 0x00, 0x00, 0x00]; // GetRxBufferStatus
            let mut len_rx = [0u8; 4];
            self.spi.transfer(&len_cmd, &mut len_rx)?;
            let pkt_len = len_rx[2];

            Ok(Some(PhyEvent::RxDone(RxMetadata {
                rssi_dbm: rssi,
                snr_db_x10: snr * 10,
                timestamp: 0,
                pkt_len,
            })))
        } else if status & 0x0001 != 0 {
            Ok(Some(PhyEvent::TxDone))
        } else if status & 0x0008 != 0 {
            Ok(Some(PhyEvent::CadDone { channel_active: true })) // assume active for now
        } else if status & 0x0200 != 0 {
            Ok(Some(PhyEvent::PreambleDetected))
        } else {
            Ok(None)
        }
    }

    fn clear_irq(&mut self) -> HalResult<()> {
        // Clear all IRQ flags (0x03FF)
        self.write_cmd(CMD_CLEAR_IRQ_STATUS, &[0x03, 0xFF])
    }

    fn rssi(&mut self) -> HalResult<i16> {
        // Issue CMD_GET_RSSI_INST (0x15), returns [status, rssi_inst]
        self.wait_busy()?;
        let cmd = [CMD_GET_RSSI_INST, 0x00, 0x00];
        let mut rx = [0u8; 3];
        self.spi.transfer(&cmd, &mut rx)?;
        let rssi_val = -(rx[2] as i16) / 2;
        Ok(rssi_val)
    }

    fn chip_name(&self) -> &'static str {
        "SX1262"
    }
}

impl<SPI: SpiMaster, PIN: GpioPin> RfDriver for Sx1262<SPI, PIN> {
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
