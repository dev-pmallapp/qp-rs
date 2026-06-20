//! I2C (Inter-Integrated Circuit) abstraction
//!
//! The canonical bus trait is re-exported from [`embedded_hal::i2c`]:
//! - [`I2c`] — blocking write, read, write_read, and transaction operations
//!
//! Platform crates implement `I2c` for their concrete bus types.  The legacy
//! [`I2cMaster`] / [`I2cDevice`] traits below are **deprecated** and will be
//! removed in a future release.

use crate::error::HalResult;

// ---------------------------------------------------------------------------
// Re-exports from embedded-hal
// ---------------------------------------------------------------------------
pub use embedded_hal::i2c::{
    ErrorType as I2cErrorType,
    I2c,
    Operation,
    SevenBitAddress,
    TenBitAddress,
};

// ---------------------------------------------------------------------------
// Configuration helpers (not part of embedded-hal)
// ---------------------------------------------------------------------------

/// I2C address (7-bit or 10-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cAddress {
    SevenBit(u8),
    TenBit(u16),
}

/// I2C speed mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cSpeed {
    /// Standard mode (100 kHz)
    Standard,
    /// Fast mode (400 kHz)
    Fast,
    /// Fast mode plus (1 MHz)
    FastPlus,
    /// High speed mode (3.4 MHz)
    HighSpeed,
}

/// I2C configuration (used by platform `configure()` extension methods)
#[derive(Debug, Clone)]
pub struct I2cConfig {
    pub speed: I2cSpeed,
}

impl Default for I2cConfig {
    fn default() -> Self {
        Self {
            speed: I2cSpeed::Standard,
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy traits — DEPRECATED; implement embedded_hal::i2c::I2c instead
// ---------------------------------------------------------------------------

/// I2C master trait
///
/// # Deprecated
/// Implement [`embedded_hal::i2c::I2c`] instead.
#[deprecated(since = "0.2.0", note = "implement embedded_hal::i2c::I2c instead")]
pub trait I2cMaster: Send + Sync {
    /// Configure I2C parameters
    fn configure(&mut self, config: &I2cConfig) -> HalResult<()>;

    /// Write data to slave
    fn write(&mut self, address: I2cAddress, data: &[u8]) -> HalResult<()>;

    /// Read data from slave
    fn read(&mut self, address: I2cAddress, buffer: &mut [u8]) -> HalResult<()>;

    /// Write then read (restart condition)
    fn write_read(
        &mut self,
        address: I2cAddress,
        write_data: &[u8],
        read_buffer: &mut [u8],
    ) -> HalResult<()>;
}

/// I2C device (master + address)
///
/// # Deprecated
/// Use [`embedded_hal::i2c::I2c`] together with the device address directly.
#[deprecated(since = "0.2.0", note = "use embedded_hal::i2c::I2c with the device address directly")]
pub trait I2cDevice: Send + Sync {
    /// Get device address
    fn address(&self) -> I2cAddress;

    /// Write data to this device
    fn write(&mut self, data: &[u8]) -> HalResult<()>;

    /// Read data from this device
    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()>;

    /// Write then read from this device
    fn write_read(&mut self, write_data: &[u8], read_buffer: &mut [u8]) -> HalResult<()>;
}
