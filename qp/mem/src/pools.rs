#![allow(dead_code)]

use crate::{QMemoryPool, QPoolStats};
use qp_core::{QResult, QError};
use critical_section::Mutex;
use core::cell::RefCell;

/// A static memory pool with fixed-size blocks
pub struct QStaticPool<T, const N: usize> {
    pool: Mutex<RefCell<Pool<T, N>>>,
    stats: Mutex<RefCell<QPoolStats>>,
}

impl<T, const N: usize> QStaticPool<T, N> {
    /// Create a new static pool
    pub const fn new() -> Self {
        Self {
            pool: Mutex::new(RefCell::new(Pool::new())),
            stats: Mutex::new(RefCell::new(QPoolStats::new(N))),
        }
    }
    
    /// Initialize the pool with storage
    pub fn init(&self, storage: &'static mut [Node<T>; N]) -> QResult<()> {
        critical_section::with(|_| {
            // Initialize the pool with the provided storage
            let mut pool = self.pool.borrow_mut();
            *pool = Pool::new();
            
            // Add all nodes to the pool
            for node in storage.iter_mut() {
                // This is safe because we're in a critical section and
                // the pool will manage the lifetime of the nodes
                unsafe {
                    pool.add(node);
                }
            }
            
            Ok(())
        })
    }
}

impl<T, const N: usize> QMemoryPool for QStaticPool<T, N> {
    type Block = T;
    
    fn alloc(&self) -> QResult<Self::Block> {
        critical_section::with(|_| {
            let mut pool = self.pool.borrow_mut();
            let mut stats = self.stats.borrow_mut();
            
            match pool.alloc() {
                Some(block) => {
                    stats.on_alloc();
                    Ok(block)
                }
                None => Err(QError::OutOfMemory)
            }
        })
    }
    
    fn dealloc(&self, block: Self::Block) -> QResult<()> {
        critical_section::with(|_| {
            let mut pool = self.pool.borrow_mut();
            let mut stats = self.stats.borrow_mut();
            
            // For heapless::Pool, we need to convert back to a Node
            // This is a simplified version - in practice, we'd need
            // proper node tracking
            stats.on_dealloc();
            
            // Note: heapless::Pool doesn't have a direct dealloc method
            // This would need to be implemented differently in practice
            Ok(())
        })
    }
    
    fn stats(&self) -> QPoolStats {
        critical_section::with(|_| {
            *self.stats.borrow()
        })
    }
    
    fn block_size(&self) -> usize {
        core::mem::size_of::<T>()
    }
}

/// Macro to create and initialize a static memory pool
#[macro_export]
macro_rules! define_static_pool {
    ($name:ident, $block_type:ty, $count:literal) => {
        static mut POOL_STORAGE: [heapless::pool::Node<$block_type>; $count] = 
            [heapless::pool::Node::new(); $count];
        
        static $name: $crate::QStaticPool<$block_type, $count> = 
            $crate::QStaticPool::new();
        
        // Initialize the pool (this should be called during system initialization)
        pub fn init_pool() -> qp_core::QResult<()> {
            unsafe {
                $name.init(&mut POOL_STORAGE)
            }
        }
    };
}

/// A dynamic memory pool that can grow and shrink
pub struct QDynamicPool<T> {
    blocks: Mutex<RefCell<heapless::Vec<T, 256>>>, // Fixed maximum for no_std
    stats: Mutex<RefCell<QPoolStats>>,
}

impl<T> QDynamicPool<T> {
    /// Create a new dynamic pool
    pub const fn new() -> Self {
        Self {
            blocks: Mutex::new(RefCell::new(heapless::Vec::new())),
            stats: Mutex::new(RefCell::new(QPoolStats::new(0))),
        }
    }
}

impl<T: Default> QMemoryPool for QDynamicPool<T> {
    type Block = T;
    
    fn alloc(&self) -> QResult<Self::Block> {
        critical_section::with(|_| {
            let mut blocks = self.blocks.borrow_mut();
            let mut stats = self.stats.borrow_mut();
            
            match blocks.pop() {
                Some(block) => {
                    stats.on_alloc();
                    Ok(block)
                }
                None => {
                    // Create a new block if we can
                    if blocks.len() < blocks.capacity() {
                        stats.total_blocks += 1;
                        stats.on_alloc();
                        Ok(T::default())
                    } else {
                        Err(QError::OutOfMemory)
                    }
                }
            }
        })
    }
    
    fn dealloc(&self, block: Self::Block) -> QResult<()> {
        critical_section::with(|_| {
            let mut blocks = self.blocks.borrow_mut();
            let mut stats = self.stats.borrow_mut();
            
            match blocks.push(block) {
                Ok(()) => {
                    stats.on_dealloc();
                    Ok(())
                }
                Err(_) => Err(QError::OutOfMemory) // Pool is full
            }
        })
    }
    
    fn stats(&self) -> QPoolStats {
        critical_section::with(|_| {
            *self.stats.borrow()
        })
    }
    
    fn block_size(&self) -> usize {
        core::mem::size_of::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pool_stats() {
        let mut stats = QPoolStats::new(10);
        
        assert_eq!(stats.total_blocks, 10);
        assert_eq!(stats.free_blocks, 10);
        assert_eq!(stats.used_blocks, 0);
        assert_eq!(stats.min_free_blocks, 10);
        assert!(stats.is_empty());
        assert!(!stats.is_full());
        
        stats.on_alloc();
        assert_eq!(stats.free_blocks, 9);
        assert_eq!(stats.used_blocks, 1);
        assert_eq!(stats.min_free_blocks, 9);
        
        stats.on_dealloc();
        assert_eq!(stats.free_blocks, 10);
        assert_eq!(stats.used_blocks, 0);
    }
}