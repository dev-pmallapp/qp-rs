//! QS user record type identifiers for the comms crate.
//!
//! IDs 100–127 are reserved for qp-rs application records.

/// PHY: frame queued for TX (freq, sf, bw, power, frame bytes).
pub const RF_PHY_TX:        u8 = 110;
/// PHY: TX on-air complete (from ISR bridge; wall-clock timestamp).
pub const RF_PHY_TX_DONE:   u8 = 111;
/// PHY: RX frame captured (rssi, snr, pkt_len, raw bytes).
pub const RF_PHY_RX:        u8 = 112;
/// MAC: LoRaWAN frame built — DevAddr, FCnt, MIC (4 bytes).
pub const RF_MAC_FRAME:     u8 = 113;
/// MAC: incoming frame validated (or dropped) — DevAddr, FCnt, pass/fail.
pub const RF_MAC_PARSE:     u8 = 114;
/// Network: port dispatch resolved (port → signal).
pub const RF_NET_ROUTE:     u8 = 115;
/// Transport: PDU enqueued with SEQ, flags, payload length.
pub const RF_TRANSPORT_TX:  u8 = 116;
/// Transport: ACK received — SEQ, round-trip ticks.
pub const RF_TRANSPORT_ACK: u8 = 117;
/// Transport: retransmit attempt — SEQ, attempt count.
pub const RF_TRANSPORT_RET: u8 = 118;
/// FOTA: chunk sent — chunk index, total chunks.
pub const FOTA_CHUNK:       u8 = 119;
/// PHY: `RF_RX_START_SIG` processed — payload tag `1` if it armed continuous
/// RX (state was `Idle`), tag `0` if it was a no-op (state was busy).
/// Diagnostic for Stage 1.5 (`docs/03-design/DES_multi_oht_channel_access.md`
/// in swm-rs).
pub const RF_RX_ARMED:      u8 = 109;

// ── Compatibility Aliases ───────────────────────────────────────────────────
pub const LORA_TX_PKT:  u8 = RF_PHY_TX;
pub const LORA_TX_DONE: u8 = RF_PHY_TX_DONE;
