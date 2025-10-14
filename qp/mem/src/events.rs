#![allow(dead_code)]

use qp_core::{QEvent, QSignal, QResult, QError};
// use crate::QMemoryPool; // Temporarily disabled until pools are re-enabled
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};

/// Reference-counted event wrapper for automatic memory management
pub struct QEvt {
    /// Pointer to the event data
    event: NonNull<dyn QEvent>,
    /// Reference count for automatic cleanup
    ref_count: AtomicU32,
    /// Pool from which this event was allocated
    pool_id: u8,
}

impl QEvt {
    /// Create a new event wrapper
    pub fn new(event: NonNull<dyn QEvent>, pool_id: u8) -> Self {
        Self {
            event,
            ref_count: AtomicU32::new(1),
            pool_id,
        }
    }
    
    /// Get the event reference
    pub fn event(&self) -> &dyn QEvent {
        // SAFETY: The event pointer is guaranteed to be valid
        // for the lifetime of this QEvt instance
        unsafe { self.event.as_ref() }
    }
    
    /// Increment the reference count
    pub fn clone_ref(&self) -> QEvt {
        self.ref_count.fetch_add(1, Ordering::AcqRel);
        QEvt {
            event: self.event,
            ref_count: AtomicU32::new(0), // Cloned reference doesn't own the count
            pool_id: self.pool_id,
        }
    }
    
    /// Get the reference count
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::Acquire)
    }
    
    /// Get the pool ID
    pub fn pool_id(&self) -> u8 {
        self.pool_id
    }
    
    /// Check if this is the last reference
    pub fn is_last_ref(&self) -> bool {
        self.ref_count.load(Ordering::Acquire) == 1
    }
}

impl Drop for QEvt {
    fn drop(&mut self) {
        let count = self.ref_count.fetch_sub(1, Ordering::AcqRel);
        if count == 1 {
            // Last reference - deallocate the event
            // Note: In a real implementation, we would call back to the memory manager
            // to return this event to its pool
        }
    }
}

// Temporarily disabled until QMemoryPool is re-enabled
/*
/// Event allocator for creating events from memory pools
pub struct QEventAllocator<'a, P: QMemoryPool> {
    pool: &'a P,
    pool_id: u8,
}

impl<'a, P: QMemoryPool> QEventAllocator<'a, P> {
    /// Create a new event allocator
    pub fn new(pool: &'a P, pool_id: u8) -> Self {
        Self { pool, pool_id }
    }
    
    /// Allocate an event of the specified type
    pub fn alloc<E>(&self, signal: QSignal, data: E) -> QResult<QEvt>
    where
        E: QEvent + 'static,
    {
        // Check if the event fits in the pool's block size
        if core::mem::size_of::<E>() > self.pool.block_size() {
            return Err(QError::InvalidSize);
        }
        
        // Allocate from the pool
        let block = self.pool.alloc()?;
        
        // In a real implementation, we would:
        // 1. Cast the block to the appropriate type
        // 2. Initialize it with the event data
        // 3. Create a QEvt wrapper
        
        // For now, return a placeholder error
        Err(QError::OutOfMemory)
    }
    
    /// Allocate a simple signal event (no data payload)
    pub fn alloc_signal(&self, signal: QSignal) -> QResult<QEvt> {
        self.alloc(signal, SignalEvent { signal })
    }
}
*/

/// Simple signal event with no data payload
#[derive(Debug, Clone, Copy)]
pub struct SignalEvent {
    signal: QSignal,
}

impl QEvent for SignalEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

/// Event factory for creating different types of events
pub struct QEventFactory {
    small_pool_id: u8,
    medium_pool_id: u8,
    large_pool_id: u8,
}

impl QEventFactory {
    /// Create a new event factory
    pub const fn new(small_pool_id: u8, medium_pool_id: u8, large_pool_id: u8) -> Self {
        Self {
            small_pool_id,
            medium_pool_id,
            large_pool_id,
        }
    }
    
    /// Create a signal event (no data)
    pub fn signal(&self, signal: QSignal) -> QResult<QEvt> {
        // Use the smallest pool for signal events
        self.create_event(self.small_pool_id, SignalEvent { signal })
    }
    
    /// Create an event with data payload
    pub fn event<E>(&self, signal: QSignal, data: E) -> QResult<QEvt>
    where
        E: QEvent + 'static,
    {
        let size = core::mem::size_of::<E>();
        
        // Select pool based on size
        let pool_id = if size <= 16 {
            self.small_pool_id
        } else if size <= 64 {
            self.medium_pool_id
        } else if size <= 256 {
            self.large_pool_id
        } else {
            return Err(QError::InvalidSize);
        };
        
        self.create_event(pool_id, data)
    }
    
    /// Internal method to create an event from a specific pool
    fn create_event<E>(&self, _pool_id: u8, _event: E) -> QResult<QEvt>
    where
        E: QEvent + 'static,
    {
        // In a real implementation, this would:
        // 1. Look up the pool by ID
        // 2. Allocate memory from that pool
        // 3. Initialize the event in the allocated memory
        // 4. Return a QEvt wrapper
        
        // For now, return a placeholder error
        Err(QError::OutOfMemory)
    }
}

/// Garbage collector for automatic event cleanup
pub struct QEventGC {
    /// Events scheduled for cleanup
    pending_cleanup: heapless::Vec<QEvt, 64>,
}

impl QEventGC {
    /// Create a new garbage collector
    pub const fn new() -> Self {
        Self {
            pending_cleanup: heapless::Vec::new(),
        }
    }
    
    /// Schedule an event for cleanup
    pub fn schedule_cleanup(&mut self, event: QEvt) -> QResult<()> {
        self.pending_cleanup
            .push(event)
            .map_err(|_| QError::OutOfMemory)
    }
    
    /// Run garbage collection cycle
    pub fn collect(&mut self) -> usize {
        let initial_count = self.pending_cleanup.len();
        
        // Remove events that are ready for cleanup
        self.pending_cleanup.retain(|evt| {
            // If reference count is 1, it's ready for cleanup
            evt.ref_count() > 1
        });
        
        initial_count - self.pending_cleanup.len()
    }
    
    /// Get the number of events pending cleanup
    pub fn pending_count(&self) -> usize {
        self.pending_cleanup.len()
    }
    
    /// Force cleanup of all events (for shutdown)
    pub fn force_cleanup(&mut self) {
        self.pending_cleanup.clear();
    }
}

/// Macro to define a custom event type
#[macro_export]
macro_rules! define_event {
    ($name:ident, $signal:expr, { $($field:ident: $type:ty),* }) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            $(pub $field: $type,)*
        }
        
        impl qp_core::QEvent for $name {
            fn signal(&self) -> qp_core::QSignal {
                $signal
            }
        }
    };
    
    // Version without fields (signal-only event)
    ($name:ident, $signal:expr) => {
        #[derive(Debug, Clone, Copy)]
        pub struct $name;
        
        impl qp_core::QEvent for $name {
            fn signal(&self) -> qp_core::QSignal {
                $signal
            }
        }
    };
}

// Tests temporarily disabled until memory pool implementation is complete
/*
#[cfg(test)]
mod tests {
    use super::*;
    use qp_core::define_signals;
    
    define_signals! {
        TEST_SIGNAL = 1,
        DATA_SIGNAL = 2,
    }
    
    define_event!(TestEvent, TEST_SIGNAL);
    define_event!(DataEvent, DATA_SIGNAL, {
        value: u32,
        message: [u8; 16]
    });
    
    #[test]
    fn test_event_creation() {
        let signal_evt = TestEvent;
        assert_eq!(signal_evt.signal(), TEST_SIGNAL);
        assert_eq!(signal_evt.size(), core::mem::size_of::<TestEvent>());
        
        let data_evt = DataEvent {
            value: 42,
            message: [0; 16],
        };
        assert_eq!(data_evt.signal(), DATA_SIGNAL);
        assert!(data_evt.size() > signal_evt.size());
    }
    
    #[test]
    fn test_garbage_collector() {
        let mut gc = QEventGC::new();
        assert_eq!(gc.pending_count(), 0);
        
        // Note: Would need actual QEvt instances to test properly
        let collected = gc.collect();
        assert_eq!(collected, 0);
    }
}
*/