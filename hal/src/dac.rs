//! DAC (Digital-to-Analog Converter) abstraction

use crate::error::HalResult;

/// DAC resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DacResolution {
    Bits8,
    Bits10,
    Bits12,
    Bits16,
}

/// DAC configuration
#[derive(Debug, Clone)]
pub struct DacConfig {
    pub resolution: DacResolution,
}

impl Default for DacConfig {
    fn default() -> Self {
        Self {
            resolution: DacResolution::Bits12,
        }
    }
}

/// DAC channel trait
pub trait DacChannel: Send + Sync {
    /// Write raw DAC value
    fn write_raw(&mut self, value: u16) -> HalResult<()>;

    /// Write voltage in millivolts
    fn write_millivolts(&mut self, millivolts: u32) -> HalResult<()>;

    /// Enable DAC output
    fn enable(&mut self) -> HalResult<()>;

    /// Disable DAC output
    fn disable(&mut self) -> HalResult<()>;

    /// Get channel number
    fn channel_number(&self) -> u8;
}
