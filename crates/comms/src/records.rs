//! QS user record type identifiers for the comms crate.
//!
//! IDs 100–127 are reserved for qp-rs application records.
//! Register each with the host via `port.emit_usr_dict(id, name)` at startup.

/// LoRa TX packet: full LoRaWAN uplink frame + RF config fields.
pub const LORA_TX_PKT:  u8 = 110;
/// LoRa TX done: acknowledgement after on-air completion (future interrupt path).
pub const LORA_TX_DONE: u8 = 111;
/// FOTA chunk sent over RF.
pub const FOTA_CHUNK:   u8 = 112;
