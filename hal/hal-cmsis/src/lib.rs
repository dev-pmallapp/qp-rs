//! ARM CMSIS HAL implementation
//!
//! Hardware abstraction layer implementation for ARM Cortex-M devices using CMSIS.

#![no_std]

pub mod asm;
pub mod basepri;
pub mod nvic;
pub mod systick;
pub mod scb;

#[cfg(feature = "stm32f4xx")]
pub mod stm32f4;

#[cfg(feature = "nrf52840")]
pub mod nrf52;

#[cfg(feature = "lpc1768")]
pub mod lpc17;

// Placeholder modules for not-yet-implemented chip variants
#[cfg(feature = "cc26xx")]
pub mod cc26xx {}

#[cfg(feature = "ht32f5")]
pub mod ht32f5 {}
