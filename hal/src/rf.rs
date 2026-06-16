//! Radio Physical Layer (PHY) Abstraction
//!
//! Provides a hardware-agnostic trait for RF transceiver devices.
//! This module does not depend on the QP-RS framework, allowing it to be used
//! in any embedded environment.

use crate::error::HalResult;
use crate::lora::LoRaModulation;

/// Requested radio operating mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMode {
    /// Lowest power state.
    Sleep,
    /// Intermediate power state with SPI register access available.
    Standby,
    /// RX mode (with optional timeout in milliseconds).
    Rx { timeout_ms: Option<u32> },
    /// Single packet transmission mode.
    Tx,
    /// Channel Activity Detection (LoRa only).
    Cad,
}

/// Metadata captured by the radio at the time of frame reception.
#[derive(Debug, Clone, Copy, Default)]
pub struct RxMetadata {
    /// Received Signal Strength Indicator (dBm).
    pub rssi_dbm: i16,
    /// Signal-to-Noise Ratio (multiplied by 10 for integer storage, e.g. 7.5 dB -> 75).
    pub snr_db_x10: i16,
    /// Timestamp of arrival (based on hardware clock or tick timer).
    pub timestamp: u32,
    /// Length of the received packet payload in bytes.
    pub pkt_len: u8,
}

/// Asynchronous events signaled by the radio hardware.
#[derive(Debug, Clone, Copy)]
pub enum PhyEvent {
    /// Transmission completed successfully.
    TxDone,
    /// A packet was successfully received.
    RxDone(RxMetadata),
    /// RX timeout window expired.
    RxTimeout,
    /// Packet failed validation check.
    CrcError,
    /// Channel activity detection completed.
    CadDone {
        /// True if channel activity was detected.
        channel_active: bool,
    },
    /// Preamble detected on air.
    PreambleDetected,
}

/// Radio modulation parameters.
#[derive(Debug, Clone)]
pub enum RadioParams {
    /// LoRa modulation parameters.
    LoRa(LoRaModulation),
}

/// Radio transmit configuration.
#[derive(Debug, Clone)]
pub struct RfTxConfig {
    /// Frequency in Hz.
    pub frequency_hz: u32,
    /// Transmit power in dBm.
    pub tx_power_dbm: i8,
    /// Modulation parameters.
    pub params: RadioParams,
}

/// Radio receive configuration.
#[derive(Debug, Clone)]
pub struct RfRxConfig {
    /// Frequency in Hz.
    pub frequency_hz: u32,
    /// RX timeout in milliseconds.
    pub timeout_ms: Option<u32>,
    /// Modulation parameters.
    pub params: RadioParams,
}

/// Generic Physical Layer (PHY) Radio Trait.
///
/// Implemented by low-level radio chip drivers (e.g., SX1276, SX1262).
/// This interface works with raw byte buffers and is completely independent
/// of the protocol stack and OS.
pub trait RfPhy: Send {
    /// Initialize the radio transceiver hardware (reset, SPI verification, basic registers).
    fn init(&mut self) -> HalResult<()>;

    /// Set the radio operating mode.
    fn set_mode(&mut self, mode: RadioMode) -> HalResult<()>;

    /// Configure the radio parameters for transmission.
    fn configure_tx(&mut self, cfg: &RfTxConfig) -> HalResult<()>;

    /// Configure the radio parameters for reception.
    fn configure_rx(&mut self, cfg: &RfRxConfig) -> HalResult<()>;

    /// Place a raw payload into the radio's buffer and trigger transmission.
    ///
    /// This is non-blocking: it queues the transfer and returns immediately.
    /// An asynchronous event (e.g. `PhyEvent::TxDone`) will signal completion.
    fn transmit(&mut self, payload: &[u8]) -> HalResult<()>;

    /// Read the received packet data from the hardware FIFO or DMA buffer.
    ///
    /// Must be called after receiving a `PhyEvent::RxDone`.
    fn read_rx(&mut self, buf: &mut [u8], meta: &RxMetadata) -> HalResult<()>;

    /// Poll the radio's IRQ status (useful on hosts or ports without GPIO interrupt mappings).
    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>>;

    /// Clear all pending interrupts on the hardware transceiver.
    fn clear_irq(&mut self) -> HalResult<()>;

    /// Measure the instantaneous RSSI (dBm) on the configured frequency.
    fn rssi(&mut self) -> HalResult<i16>;

    /// Get a human-readable identifier for the transceiver chip (e.g., "SX1262").
    fn chip_name(&self) -> &'static str;
}
