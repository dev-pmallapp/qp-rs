//! ADC (Analog-to-Digital Converter) abstraction

use crate::error::HalResult;

/// ADC resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcResolution {
    Bits8,
    Bits10,
    Bits12,
    Bits16,
}

/// ADC reference voltage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdcReference {
    /// Internal reference
    Internal,
    /// External reference on VREF pin
    External,
    /// VCC/VDD as reference
    Vcc,
}

/// ADC configuration
#[derive(Debug, Clone)]
pub struct AdcConfig {
    pub resolution: AdcResolution,
    pub reference: AdcReference,
}

impl Default for AdcConfig {
    fn default() -> Self {
        Self {
            resolution: AdcResolution::Bits12,
            reference: AdcReference::Vcc,
        }
    }
}

/// ADC channel trait
pub trait AdcChannel: Send + Sync {
    /// Read raw ADC value
    fn read_raw(&mut self) -> HalResult<u16>;

    /// Read voltage in millivolts
    fn read_millivolts(&mut self) -> HalResult<u32>;

    /// Get channel number
    fn channel_number(&self) -> u8;
}

/// ADC controller trait
pub trait AdcController: Send + Sync {
    /// ADC channel type
    type Channel: AdcChannel;

    /// Configure ADC
    fn configure(&mut self, config: &AdcConfig) -> HalResult<()>;

    /// Get channel by number
    fn get_channel(&self, channel: u8) -> HalResult<Self::Channel>;

    /// Start continuous conversion
    fn start_continuous(&mut self) -> HalResult<()>;

    /// Stop continuous conversion
    fn stop_continuous(&mut self) -> HalResult<()>;
}
