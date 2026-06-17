//! ESP32-C6 Radio Module
//!
//! Provides drivers for external radio transceivers.

pub type Sx1262<SPI> = hal::drivers::radio::Sx1262<SPI, crate::esp32c6::Esp32C6Pin>;
