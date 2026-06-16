//! Reusable, MCU-agnostic external device drivers.
//!
//! Drivers in this module communicate with hardware peripherals via standard
//! `hal` traits (e.g. `SpiMaster`, `GpioPin`) and are portable to any MCU target.

pub mod radio;
