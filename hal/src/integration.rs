//! QP-RS integration layer
//!
//! This module provides integration with the QP-RS real-time framework,
//! enabling HAL peripherals to post events to active objects from ISR context.

use crate::error::{HalError, HalResult};
use qf::event::{DynEvent, Signal};
use qf::ActiveObjectId;
use qk::QkKernel;

#[cfg(not(feature = "std"))]
use alloc::sync::Arc;
#[cfg(feature = "std")]
use std::sync::Arc;

/// Trait for peripherals that can post events to active objects
pub trait EventEmitter {
    /// Post event to target active object
    /// Should be called from ISR context with scheduler locked
    fn post_event(&self, target: ActiveObjectId, signal: Signal, event: DynEvent)
        -> HalResult<()>;
}

/// Peripheral with QK integration capability
pub trait QkIntegrated: Send + Sync {
    /// Set event posting callback
    fn set_event_poster(&mut self, poster: Arc<dyn EventEmitter>);

    /// Lock scheduler (called before ISR event posting)
    fn lock_scheduler(&self);

    /// Unlock scheduler (called after ISR event posting)
    fn unlock_scheduler(&self);
}

/// Helper for creating ISR event posters that integrate with QK kernel
pub struct KernelEventPoster {
    kernel: Arc<QkKernel>,
}

impl KernelEventPoster {
    /// Create new event poster from QK kernel
    pub fn new(kernel: Arc<QkKernel>) -> Self {
        Self { kernel }
    }

    /// Get reference to the kernel
    pub fn kernel(&self) -> &Arc<QkKernel> {
        &self.kernel
    }
}

impl EventEmitter for KernelEventPoster {
    fn post_event(
        &self,
        target: ActiveObjectId,
        _signal: Signal,
        event: DynEvent,
    ) -> HalResult<()> {
        // Lock scheduler at maximum priority
        let status = self.kernel.lock_scheduler(255);

        // Post event (non-blocking)
        let result = self
            .kernel
            .post(target, event)
            .map_err(|_| HalError::EventPostFailed);

        // Unlock scheduler (triggers dispatch if needed)
        self.kernel.unlock_scheduler(status);

        result
    }
}
