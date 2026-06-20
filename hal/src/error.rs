//! Common error types for HAL operations

use core::fmt;

/// HAL operation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HalError {
    /// Invalid parameter provided
    InvalidParameter,
    /// Operation not supported by this implementation
    NotSupported,
    /// Peripheral is busy
    Busy,
    /// Operation timed out
    Timeout,
    /// Hardware error occurred
    HardwareError,
    /// Configuration error
    ConfigurationError,
    /// Failed to post event to active object
    EventPostFailed,
    /// Vendor-specific error code
    VendorError(i32),
}

impl fmt::Display for HalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParameter => write!(f, "invalid parameter"),
            Self::NotSupported => write!(f, "operation not supported"),
            Self::Busy => write!(f, "peripheral busy"),
            Self::Timeout => write!(f, "operation timeout"),
            Self::HardwareError => write!(f, "hardware error"),
            Self::ConfigurationError => write!(f, "configuration error"),
            Self::EventPostFailed => write!(f, "failed to post event"),
            Self::VendorError(code) => write!(f, "vendor error code: {}", code),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for HalError {}

/// Result type for HAL operations
pub type HalResult<T> = Result<T, HalError>;

// ---------------------------------------------------------------------------
// embedded-hal 1.0 error trait impls
// ---------------------------------------------------------------------------

impl embedded_hal::spi::Error for HalError {
    fn kind(&self) -> embedded_hal::spi::ErrorKind {
        match self {
            Self::Busy          => embedded_hal::spi::ErrorKind::Other,
            Self::Timeout       => embedded_hal::spi::ErrorKind::Other,
            Self::HardwareError => embedded_hal::spi::ErrorKind::Other,
            _                   => embedded_hal::spi::ErrorKind::Other,
        }
    }
}

impl embedded_hal::i2c::Error for HalError {
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        match self {
            Self::Busy          => embedded_hal::i2c::ErrorKind::Bus,
            Self::Timeout       => embedded_hal::i2c::ErrorKind::NoAcknowledge(
                embedded_hal::i2c::NoAcknowledgeSource::Unknown,
            ),
            Self::HardwareError => embedded_hal::i2c::ErrorKind::Bus,
            _                   => embedded_hal::i2c::ErrorKind::Other,
        }
    }
}

impl embedded_hal::digital::Error for HalError {
    fn kind(&self) -> embedded_hal::digital::ErrorKind {
        embedded_hal::digital::ErrorKind::Other
    }
}

impl embedded_io::Error for HalError {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            Self::Timeout       => embedded_io::ErrorKind::TimedOut,
            Self::Busy          => embedded_io::ErrorKind::Other,
            Self::HardwareError => embedded_io::ErrorKind::Other,
            _                   => embedded_io::ErrorKind::Other,
        }
    }
}
