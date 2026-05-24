//! LoRa radio driver abstraction

use crate::error::HalResult;

/// LoRa spreading factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpreadingFactor {
    SF7  = 7,
    SF8  = 8,
    SF9  = 9,
    SF10 = 10,
    SF11 = 11,
    SF12 = 12,
}

/// LoRa signal bandwidth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// 125 kHz
    Bw125 = 0,
    /// 250 kHz
    Bw250 = 1,
    /// 500 kHz
    Bw500 = 2,
}

/// LoRa coding rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodingRate {
    Cr45 = 1,
    Cr46 = 2,
    Cr47 = 3,
    Cr48 = 4,
}

/// Radio channel (carrier frequency).
#[derive(Debug, Clone)]
pub struct LoRaChannel {
    pub frequency_hz: u32,
}

/// LoRa modulation parameters.
#[derive(Debug, Clone)]
pub struct LoRaModulation {
    pub sf:       SpreadingFactor,
    pub bw:       Bandwidth,
    pub cr:       CodingRate,
    pub preamble: u16,
}

impl Default for LoRaModulation {
    fn default() -> Self {
        Self {
            sf:       SpreadingFactor::SF7,
            bw:       Bandwidth::Bw125,
            cr:       CodingRate::Cr45,
            preamble: 8,
        }
    }
}

/// Complete TX configuration for a single transmission.
#[derive(Debug, Clone)]
pub struct LoRaTxConfig {
    pub channel:      LoRaChannel,
    pub modulation:   LoRaModulation,
    pub tx_power_dbm: i8,
}

impl LoRaTxConfig {
    /// EU868 default: 868.1 MHz, SF7 BW125 CR4/5, +14 dBm.
    pub fn eu868_default() -> Self {
        Self {
            channel:      LoRaChannel { frequency_hz: 868_100_000 },
            modulation:   LoRaModulation::default(),
            tx_power_dbm: 14,
        }
    }
}

/// Abstraction over a LoRa radio transceiver.
///
/// Implementors exist for each chip family (SX1276, SX1262, …).
/// Swap the lowest-level driver without touching any layer above.
pub trait RfDriver: Send {
    /// One-time hardware initialisation.
    fn init(&mut self) -> HalResult<()>;

    /// Transmit `payload` bytes with the given RF configuration.
    ///
    /// Returns after queuing the TX; does not wait for air-time completion.
    fn transmit(&mut self, cfg: &LoRaTxConfig, payload: &[u8]) -> HalResult<()>;

    /// Human-readable chip identifier, e.g. `"SX1276"`.
    fn chip_name(&self) -> &'static str;
}
