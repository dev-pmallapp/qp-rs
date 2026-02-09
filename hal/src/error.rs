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
