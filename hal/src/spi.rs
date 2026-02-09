//! SPI (Serial Peripheral Interface) abstraction

use crate::error::HalResult;

/// SPI mode (clock polarity and phase)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiMode {
    /// CPOL=0, CPHA=0
    Mode0,
    /// CPOL=0, CPHA=1
    Mode1,
    /// CPOL=1, CPHA=0
    Mode2,
    /// CPOL=1, CPHA=1
    Mode3,
}

/// SPI bit order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    MsbFirst,
    LsbFirst,
}

/// SPI configuration
#[derive(Debug, Clone)]
pub struct SpiConfig {
    pub frequency: u32,
    pub mode: SpiMode,
    pub bit_order: BitOrder,
}

impl Default for SpiConfig {
    fn default() -> Self {
        Self {
            frequency: 1_000_000, // 1 MHz
            mode: SpiMode::Mode0,
            bit_order: BitOrder::MsbFirst,
        }
    }
}

/// SPI master trait
pub trait SpiMaster: Send + Sync {
    /// Configure SPI parameters
    fn configure(&mut self, config: &SpiConfig) -> HalResult<()>;

    /// Transfer data (full duplex)
    fn transfer(&mut self, tx_data: &[u8], rx_buffer: &mut [u8]) -> HalResult<()>;

    /// Write-only transfer
    fn write(&mut self, data: &[u8]) -> HalResult<()>;

    /// Read-only transfer
    fn read(&mut self, buffer: &mut [u8]) -> HalResult<()>;
}

/// SPI device (master + chip select management)
pub trait SpiDevice: Send + Sync {
    /// Execute transaction with CS assertion
    fn transaction<F, R>(&mut self, f: F) -> HalResult<R>
    where
        F: FnOnce(&mut dyn SpiMaster) -> HalResult<R>;
}
