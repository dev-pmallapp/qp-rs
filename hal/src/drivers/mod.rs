//! Reusable, MCU-agnostic external device drivers.
//!
//! Drivers in this module communicate with hardware peripherals via standard
//! `embedded-hal` traits (e.g. `SpiBus`, `OutputPin`) and are portable to any
//! MCU target.

pub mod radio;
