//! Priority management for active objects and events

use core::fmt;
use crate::{QError, QResult};

/// Type-safe priority level for active objects
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct QPriority(u8);

impl QPriority {
    /// Minimum priority level (lowest priority)
    pub const MIN: QPriority = QPriority(1);
    
    /// Maximum priority level (highest priority)  
    pub const MAX: QPriority = QPriority(255);
    
    /// Invalid priority (used for idle)
    pub const INVALID: QPriority = QPriority(0);
    
    /// Create a new priority level
    pub fn new(priority: u8) -> QResult<Self> {
        if priority == 0 {
            Err(QError::InvalidPriority)
        } else {
            Ok(QPriority(priority))
        }
    }
    
    /// Create priority without validation (const fn)
    pub const fn new_unchecked(priority: u8) -> Self {
        QPriority(priority)
    }
    
    /// Get the raw priority value
    pub const fn raw(self) -> u8 {
        self.0
    }
    
    /// Check if this priority is valid
    pub const fn is_valid(self) -> bool {
        self.0 > 0
    }
    
    /// Increment priority (higher priority)
    pub fn increment(self) -> Result<Self, QError> {
        if self.0 >= Self::MAX.0 {
            Err(QError::InvalidPriority)
        } else {
            Ok(QPriority(self.0 + 1))
        }
    }
    
    /// Decrement priority (lower priority)
    pub fn decrement(self) -> Result<Self, QError> {
        if self.0 <= Self::MIN.0 {
            Err(QError::InvalidPriority)
        } else {
            Ok(QPriority(self.0 - 1))
        }
    }
}

impl fmt::Display for QPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Priority({})", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QPriority {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Priority({})", self.0);
    }
}

/// Priority ceiling for resource access control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QPriorityCeiling {
    ceiling: QPriority,
}

impl QPriorityCeiling {
    /// Create a new priority ceiling
    pub const fn new(ceiling: QPriority) -> Self {
        Self { ceiling }
    }
    
    /// Get the ceiling priority
    pub const fn ceiling(self) -> QPriority {
        self.ceiling
    }
    
    /// Check if a priority can access this resource
    pub const fn can_access(self, priority: QPriority) -> bool {
        priority.0 <= self.ceiling.0
    }
}

/// Priority mask for efficient priority set operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QPriorityMask(u64);

impl QPriorityMask {
    /// Empty priority mask
    pub const EMPTY: Self = Self(0);
    
    /// Full priority mask (all priorities set)
    pub const FULL: Self = Self(u64::MAX);
    
    /// Create a new empty priority mask
    pub const fn new() -> Self {
        Self::EMPTY
    }
    
    /// Set a priority in the mask
    pub fn set(&mut self, priority: QPriority) {
        if priority.is_valid() && priority.0 <= 64 {
            self.0 |= 1u64 << (priority.0 - 1);
        }
    }
    
    /// Clear a priority in the mask
    pub fn clear(&mut self, priority: QPriority) {
        if priority.is_valid() && priority.0 <= 64 {
            self.0 &= !(1u64 << (priority.0 - 1));
        }
    }
    
    /// Check if a priority is set in the mask
    pub const fn is_set(&self, priority: QPriority) -> bool {
        if !priority.is_valid() || priority.0 > 64 {
            false
        } else {
            (self.0 & (1u64 << (priority.0 - 1))) != 0
        }
    }
    
    /// Check if the mask is empty
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }
    
    /// Find the highest priority set in the mask
    pub fn highest_priority(&self) -> Option<QPriority> {
        if self.is_empty() {
            None
        } else {
            // Find the most significant bit set
            let msb = 63 - self.0.leading_zeros();
            QPriority::new((msb + 1) as u8).ok()
        }
    }
    
    /// Find the lowest priority set in the mask  
    pub fn lowest_priority(&self) -> Option<QPriority> {
        if self.is_empty() {
            None
        } else {
            // Find the least significant bit set
            let lsb = self.0.trailing_zeros();
            QPriority::new((lsb + 1) as u8).ok()
        }
    }
}

impl Default for QPriorityMask {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QPriorityMask {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "PriorityMask({=u64:b})", self.0);
    }
}

/// Macro to create compile-time priority constants
#[macro_export]
macro_rules! priority {
    ($value:literal) => {
        $crate::QPriority::new_unchecked($value)
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_creation() {
        assert!(QPriority::new(0).is_err());
        assert!(QPriority::new(1).is_ok());
        assert!(QPriority::new(255).is_ok());
    }
    
    #[test] 
    fn test_priority_mask() {
        let mut mask = QPriorityMask::new();
        assert!(mask.is_empty());
        
        let p1 = QPriority::new(1).unwrap();
        let p5 = QPriority::new(5).unwrap();
        
        mask.set(p1);
        mask.set(p5);
        
        assert!(mask.is_set(p1));
        assert!(mask.is_set(p5));
        assert!(!mask.is_set(QPriority::new(3).unwrap()));
        
        assert_eq!(mask.highest_priority(), Some(p5));
        assert_eq!(mask.lowest_priority(), Some(p1));
    }
}