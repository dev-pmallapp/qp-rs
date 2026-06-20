//! GPIO (General Purpose Input/Output) abstraction
//!
//! The canonical direction-typed traits are re-exported from
//! [`embedded_hal::digital`]:
//! - [`OutputPin`]         — drive a pin high or low
//! - [`InputPin`]          — read the current level
//! - [`StatefulOutputPin`] — read back the last written level + toggle
//!
//! Platform crates implement these traits on their concrete pin types
//! (e.g. `Stm32F4Pin`, `Nrf52Pin`).  Because `embedded-hal` splits
//! direction into separate traits, a single physical pin typically implements
//! **both** `InputPin` and `OutputPin`; the caller chooses which to use based
//! on the configured mode.
//!
//! The legacy [`GpioPin`] trait below is **deprecated** and will be removed in
//! a future release.

use crate::error::HalResult;

// ---------------------------------------------------------------------------
// Re-exports from embedded-hal
// ---------------------------------------------------------------------------
pub use embedded_hal::digital::{
    ErrorType as DigitalErrorType,
    InputPin,
    OutputPin,
    StatefulOutputPin,
};

// ---------------------------------------------------------------------------
// Configuration helpers (not part of embedded-hal)
// ---------------------------------------------------------------------------

/// GPIO pin modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinMode {
    /// Input (floating)
    Input,
    /// Input with pull-up resistor
    InputPullUp,
    /// Input with pull-down resistor
    InputPullDown,
    /// Output (push-pull)
    Output,
    /// Output (open-drain)
    OutputOpenDrain,
    /// Alternate function (vendor-specific)
    Alternate(u8),
}

/// GPIO pin levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Low level (0V)
    Low,
    /// High level (VCC)
    High,
}

/// Interrupt trigger edge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// Rising edge
    Rising,
    /// Falling edge
    Falling,
    /// Both edges
    Both,
}

// ---------------------------------------------------------------------------
// Legacy traits — DEPRECATED; use embedded_hal::digital::* instead
// ---------------------------------------------------------------------------

/// GPIO pin trait (object-safe)
///
/// # Deprecated
/// Use [`embedded_hal::digital::OutputPin`] and [`embedded_hal::digital::InputPin`] instead.
#[deprecated(since = "0.2.0", note = "use embedded_hal::digital::OutputPin / InputPin instead")]
pub trait GpioPin: Send + Sync {
    /// Configure pin mode
    fn set_mode(&mut self, mode: PinMode) -> HalResult<()>;

    /// Read current level
    fn read(&self) -> HalResult<Level>;

    /// Write level (for output pins)
    fn write(&mut self, level: Level) -> HalResult<()>;

    /// Toggle output
    fn toggle(&mut self) -> HalResult<()> {
        let current = self.read()?;
        let new_level = match current {
            Level::Low => Level::High,
            Level::High => Level::Low,
        };
        self.write(new_level)
    }

    /// Get pin number
    fn pin_number(&self) -> u32;
}

/// GPIO pin with interrupt support
///
/// # Deprecated
/// Use platform-specific IRQ configuration instead.
#[deprecated(since = "0.2.0", note = "use platform-specific IRQ configuration instead")]
pub trait GpioPinInterrupt: GpioPin {
    /// Enable interrupt on edge
    fn enable_interrupt(&mut self, edge: Edge) -> HalResult<()>;

    /// Disable interrupt
    fn disable_interrupt(&mut self) -> HalResult<()>;

    /// Clear pending interrupt
    fn clear_interrupt(&mut self) -> HalResult<()>;

    /// Check if interrupt is pending
    fn is_interrupt_pending(&self) -> bool;
}

/// GPIO port (collection of pins)
///
/// # Deprecated
/// Obtain typed pins directly from the platform HAL initializer.
#[deprecated(since = "0.2.0", note = "obtain typed pins directly from the platform HAL")]
pub trait GpioPort: Send + Sync {
    /// Pin type produced by this port
    type Pin: GpioPin;

    /// Interrupt-capable pin type
    type InterruptPin: GpioPinInterrupt;

    /// Get pin by number
    fn get_pin(&self, pin: u32) -> HalResult<Self::Pin>;

    /// Get interrupt-capable pin
    fn get_interrupt_pin(&self, pin: u32) -> HalResult<Self::InterruptPin>;
}
