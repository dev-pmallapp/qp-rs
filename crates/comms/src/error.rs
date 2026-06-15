//! Error types for the comms crate.

use core::fmt;
use hal::error::HalError;

/// Errors returned by the comms stack (LoRa transport, MAC, FOTA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommsError {
    /// Nothing to receive right now.
    NothingReceived,
    /// Buffer is too small for the received frame.
    BufferTooSmall,
    /// MAC layer error (frame encoding/decoding failure).
    MacError,
    /// Hardware error from the underlying RF driver.
    Hardware(HalError),
    /// FOTA-specific error.
    Fota(FotaError),
}

/// Errors specific to firmware-over-the-air (FOTA) transfers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FotaError {
    /// Chunk index is out of range.
    ChunkOutOfRange,
    /// Image CRC/checksum mismatch after full transfer.
    ChecksumMismatch,
    /// Session not initialised.
    NotStarted,
}

impl From<HalError> for CommsError {
    fn from(e: HalError) -> Self { CommsError::Hardware(e) }
}

impl fmt::Display for CommsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NothingReceived  => write!(f, "nothing received"),
            Self::BufferTooSmall   => write!(f, "buffer too small"),
            Self::MacError         => write!(f, "MAC layer error"),
            Self::Hardware(e)      => write!(f, "hardware error: {e}"),
            Self::Fota(e)          => write!(f, "FOTA error: {e:?}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CommsError {}
