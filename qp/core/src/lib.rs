#![no_std]
#![forbid(unsafe_code)]

//! # QP Core
//! 
//! Core types, traits, and abstractions for the QP real-time embedded framework.
//! This crate provides the foundation for building event-driven, hierarchical 
//! state machine applications in Rust.

use core::fmt;

pub mod events;
pub mod states;
pub mod priorities;
pub mod time;

pub use events::*;
pub use states::*;
pub use priorities::*;
pub use time::*;

#[cfg(test)]
mod tests;

/// QP framework version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result type used throughout the QP framework
pub type QResult<T> = Result<T, QError>;

/// Error types for QP framework operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QError {
    /// Event queue is full
    QueueFull,
    /// Event queue is empty  
    QueueEmpty,
    /// Invalid priority level
    InvalidPriority,
    /// Invalid state machine transition
    InvalidTransition,
    /// Memory pool exhausted
    OutOfMemory,
    /// Invalid size for allocation
    InvalidSize,
    /// Timer operation failed
    TimerError,
    /// Generic framework error
    Framework,
}

impl fmt::Display for QError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QError::QueueFull => write!(f, "Event queue is full"),
            QError::QueueEmpty => write!(f, "Event queue is empty"),
            QError::InvalidPriority => write!(f, "Invalid priority level"),
            QError::InvalidTransition => write!(f, "Invalid state machine transition"),
            QError::OutOfMemory => write!(f, "Memory pool exhausted"),
            QError::InvalidSize => write!(f, "Invalid size for allocation"),
            QError::TimerError => write!(f, "Timer operation failed"),
            QError::Framework => write!(f, "Generic framework error"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for QError {}

#[cfg(feature = "defmt")]
impl defmt::Format for QError {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            QError::QueueFull => defmt::write!(fmt, "QueueFull"),
            QError::QueueEmpty => defmt::write!(fmt, "QueueEmpty"),
            QError::InvalidPriority => defmt::write!(fmt, "InvalidPriority"),
            QError::InvalidTransition => defmt::write!(fmt, "InvalidTransition"),
            QError::OutOfMemory => defmt::write!(fmt, "OutOfMemory"),
            QError::InvalidSize => defmt::write!(fmt, "InvalidSize"),
            QError::TimerError => defmt::write!(fmt, "TimerError"),
            QError::Framework => defmt::write!(fmt, "Framework"),
        }
    }
}