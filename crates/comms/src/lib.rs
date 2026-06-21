//! Comms — communication middleware for QP-RS.
//!
//! Provides a layered, radio-agnostic protocol stack composed at compile time.
//!
//! The crate is `no_std` + `alloc`: it builds for bare-metal targets when the
//! default `std` feature is disabled. Debug logging falls back to no-ops on
//! `no_std` (see the internal `log` macros).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[macro_use]
mod log;

pub mod buf;
pub mod error;
pub mod events;
pub mod fota;
pub mod lora;
pub mod mac;
pub mod net;
pub mod null_rf;
pub mod phy;
pub mod records;
pub mod rf;
pub mod session;
pub mod stack;
pub mod transport;

pub use error::CommsError;
pub use events::{RfTxReqPayload, RF_TX_DONE_SIG, RF_TX_REQ_SIG};
pub use fota::FotaSession;
pub use lora::LoRaRf;
pub use mac::CommsAO;
pub use null_rf::NullRf;
pub use records::{FOTA_CHUNK, LORA_TX_DONE, LORA_TX_PKT};
pub use rf::Rf;
pub use session::LoRaSession;

// New modular stack re-exports
pub use buf::{Frame, FrameIdx, FramePool};
pub use stack::{Layer, RfStack, RfStackAO};
pub use transport::{ReliableTransport, UnreliableTransport, TransportAction};
pub use net::{Network, NoopNetwork};
pub use mac::noop::NoopMac;
pub use mac::ble_l2cap::BleL2capMac;
