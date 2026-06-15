//! ESP-IDF C SDK wrapper for HAL traits.
//!
//! Use this crate when you need ESP-IDF managed subsystems (WiFi, BLE, NVS,
//! OTA, TLS) alongside QP-RS.  For bare peripheral access (GPIO, SPI, UART,
//! timer, interrupts) prefer `hal-lxsis` (ESP32/S3) or `hal-rvsis` (ESP32-C6)
//! which are pure Rust and have no build-time C toolchain requirement.
//!
//! ## When to choose this crate vs the pure-Rust alternatives
//!
//! | Need | Crate |
//! |------|-------|
//! | GPIO / SPI / UART / timer only | `hal-lxsis` / `hal-rvsis` |
//! | WiFi, BLE, NVS, OTA, TLS | `hal-esp-idf` (this crate) |
//! | Mixed: pure-Rust peripherals + ESP-IDF networking | both — `hal-rvsis` for peripherals, `hal-esp-idf` for networking |
//!
//! ## Chip support
//! Enable exactly one chip feature per build:
//! - `esp32`   — Xtensa LX6 dual-core (uses `hal-lxsis` for peripherals)
//! - `esp32s3` — Xtensa LX7 dual-core (uses `hal-lxsis` for peripherals)
//! - `esp32c6` — RISC-V RV32IMAC (uses `hal-rvsis` for peripherals)

#![no_std]

pub mod gpio;
pub mod spi;
pub mod uart;
