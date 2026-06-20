//! SPI (Serial Peripheral Interface) abstraction
//!
//! The canonical bus traits are re-exported from [`embedded_hal::spi`]:
//! - [`SpiBus`]       — full-duplex bus (owns the bus, no CS management)
//! - [`SpiDevice`]    — bus + CS management per logical device
//! - [`ErrorType`]    — associate an `Error` type with an impl
//!
//! Platform crates implement `SpiBus<u8>` (and optionally `SpiDevice`) for
//! their concrete peripheral types.  The legacy `SpiMaster` / `SpiDevice`
//! traits below are **deprecated** and will be removed in a future release.

use crate::error::HalResult;

// ---------------------------------------------------------------------------
// Re-exports from embedded-hal
// ---------------------------------------------------------------------------
pub use embedded_hal::spi::{
    ErrorType, Mode, Phase, Polarity,
    SpiBus,
    SpiDevice as EmbeddedSpiDevice,
    MODE_0, MODE_1, MODE_2, MODE_3,
};

// ---------------------------------------------------------------------------
// Configuration helpers (not part of embedded-hal; kept for platform `new()`)
// ---------------------------------------------------------------------------

/// SPI mode (clock polarity and phase)
///
/// Prefer the [`Mode`] constants from `embedded_hal::spi` (`MODE_0`..`MODE_3`)
/// for new code.  This enum remains for backward compatibility.
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

impl From<SpiMode> for Mode {
    fn from(m: SpiMode) -> Mode {
        match m {
            SpiMode::Mode0 => MODE_0,
            SpiMode::Mode1 => MODE_1,
            SpiMode::Mode2 => MODE_2,
            SpiMode::Mode3 => MODE_3,
        }
    }
}

/// SPI bit order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    MsbFirst,
    LsbFirst,
}

/// SPI configuration (used by platform `configure()` extension methods)
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

// ---------------------------------------------------------------------------
// Legacy traits — DEPRECATED; use embedded_hal::spi::SpiBus / SpiDevice
// ---------------------------------------------------------------------------

/// SPI master trait
///
/// # Deprecated
/// Implement [`embedded_hal::spi::SpiBus`] instead.
#[deprecated(since = "0.2.0", note = "implement embedded_hal::spi::SpiBus instead")]
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
///
/// # Deprecated
/// Use [`embedded_hal::spi::SpiDevice`] instead.
#[deprecated(since = "0.2.0", note = "use embedded_hal::spi::SpiDevice instead")]
pub trait SpiDeviceLegacy: Send + Sync {
    /// Execute transaction with CS assertion
    fn transaction<F, R>(&mut self, f: F) -> HalResult<R>
    where
        F: FnOnce(&mut dyn SpiMaster) -> HalResult<R>;
}
