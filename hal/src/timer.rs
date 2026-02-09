//! Timer and PWM abstraction

use crate::error::HalResult;

/// Timer mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    OneShot,
    Periodic,
}

/// Timer trait
pub trait Timer: Send + Sync {
    /// Start timer with specified period in microseconds
    fn start(&mut self, period_us: u64, mode: TimerMode) -> HalResult<()>;

    /// Stop timer
    fn stop(&mut self) -> HalResult<()>;

    /// Get current counter value
    fn counter(&self) -> u64;

    /// Enable interrupt on timeout
    fn enable_interrupt(&mut self) -> HalResult<()>;

    /// Disable interrupt
    fn disable_interrupt(&mut self) -> HalResult<()>;

    /// Clear interrupt flag
    fn clear_interrupt(&mut self) -> HalResult<()>;
}

/// PWM channel trait
pub trait PwmChannel: Send + Sync {
    /// Set duty cycle (0.0 to 1.0)
    fn set_duty(&mut self, duty: f32) -> HalResult<()>;

    /// Set frequency in Hz
    fn set_frequency(&mut self, freq_hz: u32) -> HalResult<()>;

    /// Enable PWM output
    fn enable(&mut self) -> HalResult<()>;

    /// Disable PWM output
    fn disable(&mut self) -> HalResult<()>;
}
