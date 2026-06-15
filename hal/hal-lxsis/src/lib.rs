//! Xtensa LX HAL implementation
//!
//! Hardware abstraction layer implementation for Xtensa LX devices.

#![no_std]

pub mod asm;
pub mod intlevel;
pub mod intenable;
pub mod ccompare;

#[cfg(feature = "esp32")]
pub mod esp32;

// Placeholder for ESP32-S2 variant
#[cfg(feature = "esp32s2")]
pub mod esp32s2 {}

#[cfg(feature = "esp32s3")]
pub mod esp32s3;
