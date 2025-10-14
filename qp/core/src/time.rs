//! Time management types and utilities

use core::fmt;
use crate::QError;

/// Time event counter type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QTimeEvtCtr(pub u32);

impl QTimeEvtCtr {
    /// Maximum time counter value
    pub const MAX: Self = Self(u32::MAX);
    
    /// Zero time counter
    pub const ZERO: Self = Self(0);
    
    /// Create a new time counter
    pub const fn new(ticks: u32) -> Self {
        Self(ticks)
    }
    
    /// Get the raw tick count
    pub const fn ticks(self) -> u32 {
        self.0
    }
    
    /// Check if the counter is zero
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
    
    /// Decrement the counter by one tick
    pub fn decrement(&mut self) -> bool {
        if self.0 > 0 {
            self.0 -= 1;
            self.0 == 0 // Return true if reached zero
        } else {
            false
        }
    }
    
    /// Add ticks to the counter
    pub fn add_ticks(&mut self, ticks: u32) -> Result<(), QError> {
        self.0 = self.0.checked_add(ticks).ok_or(QError::TimerError)?;
        Ok(())
    }
    
    /// Subtract ticks from the counter
    pub fn sub_ticks(&mut self, ticks: u32) -> Result<(), QError> {
        self.0 = self.0.checked_sub(ticks).ok_or(QError::TimerError)?;
        Ok(())
    }
}

impl fmt::Display for QTimeEvtCtr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ticks", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QTimeEvtCtr {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}ticks", self.0);
    }
}

/// Time event interval types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QTimeInterval {
    /// One-shot timer
    OneShot(QTimeEvtCtr),
    /// Periodic timer with interval
    Periodic(QTimeEvtCtr),
}

impl QTimeInterval {
    /// Create a one-shot timer
    pub const fn one_shot(ticks: u32) -> Self {
        Self::OneShot(QTimeEvtCtr::new(ticks))
    }
    
    /// Create a periodic timer
    pub const fn periodic(ticks: u32) -> Self {
        Self::Periodic(QTimeEvtCtr::new(ticks))
    }
    
    /// Get the tick count for this interval
    pub const fn ticks(&self) -> QTimeEvtCtr {
        match self {
            Self::OneShot(ticks) => *ticks,
            Self::Periodic(ticks) => *ticks,
        }
    }
    
    /// Check if this is a periodic timer
    pub const fn is_periodic(&self) -> bool {
        matches!(self, Self::Periodic(_))
    }
    
    /// Check if this is a one-shot timer
    pub const fn is_one_shot(&self) -> bool {
        matches!(self, Self::OneShot(_))
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QTimeInterval {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Self::OneShot(ticks) => defmt::write!(fmt, "OneShot({})", ticks),
            Self::Periodic(ticks) => defmt::write!(fmt, "Periodic({})", ticks),
        }
    }
}

/// System tick counter for framework timing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QTick(u64);

impl QTick {
    /// Zero tick
    pub const ZERO: Self = Self(0);
    
    /// Maximum tick value
    pub const MAX: Self = Self(u64::MAX);
    
    /// Create a new tick count
    pub const fn new(ticks: u64) -> Self {
        Self(ticks)
    }
    
    /// Get the raw tick value
    pub const fn raw(self) -> u64 {
        self.0
    }
    
    /// Increment the tick counter
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
    
    /// Add ticks to the counter
    pub fn add(&mut self, ticks: u64) {
        self.0 = self.0.wrapping_add(ticks);
    }
    
    /// Calculate elapsed ticks since a previous tick
    pub fn elapsed_since(self, previous: QTick) -> u64 {
        self.0.wrapping_sub(previous.0)
    }
    
    /// Check if this tick is after another tick (handles wraparound)
    pub fn is_after(self, other: QTick) -> bool {
        self.0.wrapping_sub(other.0) < u64::MAX / 2
    }
}

impl fmt::Display for QTick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tick:{}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QTick {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "tick:{}", self.0);
    }
}

/// Duration in framework ticks
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QDuration {
    ticks: u32,
}

impl QDuration {
    /// Zero duration
    pub const ZERO: Self = Self { ticks: 0 };
    
    /// Maximum duration
    pub const MAX: Self = Self { ticks: u32::MAX };
    
    /// Create duration from ticks
    pub const fn from_ticks(ticks: u32) -> Self {
        Self { ticks }
    }
    
    /// Create duration from milliseconds (assuming 1ms tick period)
    pub const fn from_millis(millis: u32) -> Self {
        Self { ticks: millis }
    }
    
    /// Create duration from seconds (assuming 1ms tick period)
    pub const fn from_secs(secs: u32) -> Self {
        Self { ticks: secs * 1000 }
    }
    
    /// Get tick count
    pub const fn ticks(&self) -> u32 {
        self.ticks
    }
    
    /// Convert to milliseconds (assuming 1ms tick period)
    pub const fn as_millis(&self) -> u32 {
        self.ticks
    }
    
    /// Convert to seconds (assuming 1ms tick period)  
    pub const fn as_secs(&self) -> u32 {
        self.ticks / 1000
    }
    
    /// Check if duration is zero
    pub const fn is_zero(&self) -> bool {
        self.ticks == 0
    }
}

impl fmt::Display for QDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.ticks)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QDuration {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}ms", self.ticks);
    }
}

/// Macro to create compile-time durations
#[macro_export]
macro_rules! duration {
    ($value:literal ms) => {
        $crate::QDuration::from_millis($value)
    };
    ($value:literal s) => {
        $crate::QDuration::from_secs($value)  
    };
    ($value:literal ticks) => {
        $crate::QDuration::from_ticks($value)
    };
}