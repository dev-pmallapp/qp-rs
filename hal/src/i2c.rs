//! I2C (Inter-Integrated Circuit) abstraction

use crate::error::HalResult;

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

/// I2C configuration
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

/// I2C master trait
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
