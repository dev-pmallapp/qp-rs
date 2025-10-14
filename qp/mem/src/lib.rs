#![no_std]
#![allow(unsafe_code)] // Memory pools require unsafe for allocation

//! # QP Memory Management
//! 
//! Static memory pools and allocation strategies for the QP framework.
//! Provides deterministic memory management suitable for real-time embedded systems.

// Temporarily excluded from build until heapless::pool is stabilized
// pub mod pools;
pub mod events;

// pub use pools::*;
pub use events::*;

/// Memory pool statistics for debugging and monitoring
#[derive(Debug, Clone, Copy)]
pub struct QPoolStats {
    /// Total number of blocks in the pool
    pub total_blocks: usize,
    /// Number of free blocks currently available
    pub free_blocks: usize,
    /// Number of blocks currently in use
    pub used_blocks: usize,
    /// Minimum number of free blocks ever reached
    pub min_free_blocks: usize,
}

impl QPoolStats {
    /// Create new pool statistics
    pub const fn new(total_blocks: usize) -> Self {
        Self {
            total_blocks,
            free_blocks: total_blocks,
            used_blocks: 0,
            min_free_blocks: total_blocks,
        }
    }
    
    /// Update statistics after allocation
    pub fn on_alloc(&mut self) {
        self.used_blocks += 1;
        self.free_blocks -= 1;
        if self.free_blocks < self.min_free_blocks {
            self.min_free_blocks = self.free_blocks;
        }
    }
    
    /// Update statistics after deallocation
    pub fn on_dealloc(&mut self) {
        if self.used_blocks > 0 {
            self.used_blocks -= 1;
            self.free_blocks += 1;
        }
    }
    
    /// Check if the pool is full
    pub const fn is_full(&self) -> bool {
        self.free_blocks == 0
    }
    
    /// Check if the pool is empty (all blocks free)
    pub const fn is_empty(&self) -> bool {
        self.used_blocks == 0
    }
    
    /// Get utilization as a percentage (0-100)
    pub fn utilization(&self) -> u8 {
        if self.total_blocks == 0 {
            0
        } else {
            ((self.used_blocks * 100) / self.total_blocks) as u8
        }
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for QPoolStats {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(
            fmt,
            "QPoolStats{{ total: {}, free: {}, used: {}, min_free: {} }}",
            self.total_blocks,
            self.free_blocks,
            self.used_blocks,
            self.min_free_blocks
        );
    }
}

// NOTE: Memory pool implementation temporarily excluded from build
// The QMemoryPool trait and QMemoryManager will be re-enabled once
// the heapless::pool integration is properly stabilized.
// See pools.rs for the implementation (currently excluded from build).