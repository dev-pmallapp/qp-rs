//! Signals and event payloads for the comms crate.

use std::vec::Vec;
use qf::event::Signal;

/// Request a single RF uplink transmission.
pub const RF_TX_REQ_SIG:  Signal = Signal(20);
/// RF transmission completed (fired by radio ISR or polling loop).
pub const RF_TX_DONE_SIG: Signal = Signal(21);
/// A FOTA firmware chunk is ready to be sent.
pub const FOTA_CHUNK_SIG: Signal = Signal(22);

/// Payload carried by [`RF_TX_REQ_SIG`].
#[derive(Debug, Clone)]
pub struct RfTxReqPayload {
    /// Application data bytes to transmit.
    pub data:  Vec<u8>,
    /// LoRaWAN FPort (1–223 for application data).
    pub fport: u8,
}

impl RfTxReqPayload {
    pub fn new(data: Vec<u8>, fport: u8) -> Self {
        Self { data, fport }
    }
}
