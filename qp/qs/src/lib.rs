#![cfg_attr(not(feature = "std"), no_std)]

//! QS - Software Tracing Infrastructure
//!
//! This module provides lightweight software tracing for debugging and monitoring
//! QP applications. The tracing is designed to have minimal runtime overhead when
//! enabled and zero overhead when disabled via feature flags.
//!
//! Key features:
//! - Zero-overhead when disabled at compile time
//! - Minimal overhead when enabled
//! - Real-time trace streaming
//! - Filtering by trace record type
//! - Circular trace buffer
//! - Support for various output channels (UART, SWO, stdout, etc.)

// Use the std or nostd module based on feature flag
#[cfg(feature = "std")]
#[path = "std.rs"]
mod impl_mod;

#[cfg(not(feature = "std"))]
#[path = "nostd.rs"]
mod impl_mod;

// Re-export everything from the implementation module
pub use impl_mod::*;

