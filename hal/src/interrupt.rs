//! Interrupt controller abstraction

use crate::error::HalResult;

/// Interrupt priority (0 = highest on most platforms)
pub type InterruptPriority = u8;

/// Interrupt controller abstraction
pub trait InterruptController: Send + Sync {
    /// Enable interrupt
    fn enable_interrupt(&mut self, irq_num: u32) -> HalResult<()>;

    /// Disable interrupt
    fn disable_interrupt(&mut self, irq_num: u32) -> HalResult<()>;

    /// Set interrupt priority
    fn set_priority(&mut self, irq_num: u32, priority: InterruptPriority) -> HalResult<()>;

    /// Check if interrupt is pending
    fn is_pending(&self, irq_num: u32) -> bool;

    /// Clear pending interrupt
    fn clear_pending(&mut self, irq_num: u32) -> HalResult<()>;
}
