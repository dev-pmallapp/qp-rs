//! Comms — communication middleware for QP-RS.
//!
//! Provides a layered communication stack:
//!
//! ```text
//! App / FOTA
//!   │  Rf::send(payload) / Rf::receive(buf)
//!   ▼
//! Rf trait  (protocol-agnostic interface)
//!   │
//!   ├── LoRaRf  (LoRaWAN Class A frame builder + session management)
//!   │     │  RfDriver::transmit()
//!   │     └── hal::lora::RfDriver  (chip-level SPI sequences)
//!   │               ├── Sx1276<SPI>
//!   │               └── Sx1262<SPI>
//!   └── NullRf  (POSIX host testing stub)
//! ```
//!
//! # FOTA
//!
//! [`FotaSession`] implements firmware-over-the-air using any [`Rf`]
//! implementation as its transport.  See [`fota`] for the protocol sketch.

pub mod error;
pub mod events;
pub mod fota;
pub mod lora;
pub mod mac;
pub mod null_rf;
pub mod records;
pub mod rf;
pub mod session;

pub use error::CommsError;
pub use events::{RfTxReqPayload, RF_TX_DONE_SIG, RF_TX_REQ_SIG};
pub use fota::FotaSession;
pub use lora::LoRaRf;
pub use mac::CommsAO;
pub use null_rf::NullRf;
pub use records::{FOTA_CHUNK, LORA_TX_DONE, LORA_TX_PKT};
pub use rf::Rf;
pub use session::LoRaSession;
