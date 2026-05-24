//! High-level RF communication trait.
//!
//! [`Rf`] abstracts over the underlying radio technology (LoRa, BLE, …).
//! The two core operations — [`Rf::send`] and [`Rf::receive`] — are
//! protocol-agnostic; callers never need to know which chip is underneath.
//!
//! ```text
//! App / FOTA
//!   │  send(payload) / receive(buf)
//!   ▼
//! Rf  (this trait)
//!   │  LoRaRf: builds LoRaWAN frames, manages session keys
//!   ▼
//! hal::lora::RfDriver  (chip-level SPI sequences)
//!   │  SX1276 / SX1262
//!   ▼
//! SPI bus
//! ```

use crate::error::CommsError;

/// High-level radio abstraction.
///
/// Implementations hide all chip-specific details and MAC-layer framing.
pub trait Rf: Send {
    /// Transmit `payload` bytes.
    ///
    /// Returns once the payload has been handed to the radio.
    /// Does **not** wait for on-air completion (non-blocking).
    fn send(&mut self, payload: &[u8]) -> Result<(), CommsError>;

    /// Attempt to receive a frame into `buf`.
    ///
    /// Returns the number of bytes written on success, or
    /// [`CommsError::NothingReceived`] when no frame is available.
    fn receive(&mut self, buf: &mut [u8]) -> Result<usize, CommsError>;

    /// Human-readable identifier for this RF implementation, e.g. `"LoRa-SX1276"`.
    fn chip_name(&self) -> &'static str;
}
