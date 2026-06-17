//! ESP32-S3 Radio Module
//!
//! Provides drivers for external radio transceivers.

pub type Sx1276<SPI> = hal::drivers::radio::Sx1276<SPI, crate::esp32s3::Esp32S3Pin>;
