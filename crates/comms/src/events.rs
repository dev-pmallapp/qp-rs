//! Signals and event payloads for the comms crate.

#[cfg(feature = "std")]
use std::vec::Vec;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use qf::event::Signal;
use hal::rf::PhyEvent;

// ── Application → RfStackAO ───────────────────────────────────────────────
pub const RF_TX_REQ_SIG:          Signal = Signal(20);
pub const RF_RX_START_SIG:        Signal = Signal(21);  // enter RX mode

// ── PHY ISR → RfStackAO (posted from port ISR bridge) ────────────────────
pub const RF_PHY_IRQ_SIG:         Signal = Signal(22);  // generic DIO fire
pub const RF_PHY_TX_DONE_SIG:     Signal = Signal(23);
pub const RF_PHY_RX_DONE_SIG:     Signal = Signal(24);
pub const RF_PHY_RX_TIMEOUT_SIG:  Signal = Signal(25);
pub const RF_PHY_CRC_ERROR_SIG:   Signal = Signal(26);

// ── RfStackAO → Application ───────────────────────────────────────────────
pub const RF_TX_DONE_SIG:         Signal = Signal(27);
pub const RF_TX_FAIL_SIG:         Signal = Signal(28);
pub const RF_RX_FRAME_SIG:        Signal = Signal(29);  // payload received

// ── Internal ──────────────────────────────────────────────────────────────
pub const RF_TRANSPORT_TIMEOUT_SIG: Signal = Signal(30);

/// Legacy / compatibility signal (aliased to RF_PHY_TX_DONE_SIG or RF_TX_DONE_SIG)
pub const RF_TX_DONE_SIG_LEGACY: Signal = Signal(21);

/// Payload carried by [`RF_TX_REQ_SIG`].
#[derive(Debug, Clone)]
pub struct RfTxReqPayload {
    /// Application data bytes to transmit.
    pub data:     Vec<u8>,
    /// LoRaWAN FPort (1–223 for application data).
    pub fport:    u8,
    /// Whether to require reliability (ACK/retransmissions).
    pub reliable: bool,
}

impl RfTxReqPayload {
    /// Creates a transmit-request payload for the given data and LoRaWAN FPort.
    pub fn new(data: Vec<u8>, fport: u8) -> Self {
        Self { data, fport, reliable: false }
    }

    /// Creates a transmit-request payload with reliability option.
    pub fn with_reliability(data: Vec<u8>, fport: u8, reliable: bool) -> Self {
        Self { data, fport, reliable }
    }
}

/// Payload for RF_RX_FRAME_SIG (application receives this).
#[derive(Debug, Clone)]
pub struct RfRxFramePayload {
    pub data:    heapless::Vec<u8, 242>,
    pub port:    u8,
    pub rssi:    i16,
    pub snr:     i16,
}

/// Payload for RF_PHY_IRQ_SIG (posted from ISR).
#[derive(Clone, Copy, Debug)]
pub struct PhyIrqPayload {
    pub event: PhyEvent,
    pub meta:  hal::rf::RxMetadata,
}
