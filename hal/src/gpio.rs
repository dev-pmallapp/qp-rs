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
