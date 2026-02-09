//! Hardware Abstraction Layer (HAL) for embedded systems
//!
//! This crate provides vendor-agnostic traits for common embedded peripherals.
//! It can be used standalone or integrated with the QP-RS real-time framework.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod error;
pub mod gpio;
pub mod uart;
pub mod spi;
pub mod i2c;
pub mod timer;
pub mod adc;
pub mod dac;
pub mod interrupt;

#[cfg(feature = "qp-integration")]
pub mod integration;

// Re-export commonly used types
pub use error::HalError;
