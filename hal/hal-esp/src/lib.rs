//! ESP-IDF HAL implementation
//!
//! Hardware abstraction layer implementation for ESP32 family using ESP-IDF.
//!
//! This crate provides concrete implementations of the HAL traits for ESP32
//! microcontrollers using the ESP-IDF framework.
//!
//! ## Supported Chips
//! - ESP32 (Xtensa dual-core)
//! - ESP32-S3 (Xtensa dual-core)
//! - ESP32-C6 (RISC-V single-core)
//!
//! ## Features
//! - `esp32` - ESP32 support
//! - `esp32s3` - ESP32-S3 support
//! - `esp32c6` - ESP32-C6 support
//! - `qp-integration` - Enable QP-RS kernel integration
//!
//! ## Example
//! ```no_run
//! use hal_esp::gpio::EspGpioPin;
//! use hal::gpio::{GpioPin, Level, PinMode};
//!
//! let mut led = EspGpioPin::new(2).unwrap();
//! led.set_mode(PinMode::Output).unwrap();
//! led.write(Level::High).unwrap();
//! ```

#![no_std]

// Re-export esp-idf-sys for users who need direct access
pub use esp_idf_sys;

pub mod gpio;

// Re-export commonly used types
pub use gpio::EspGpioPin;
