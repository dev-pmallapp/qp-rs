//! UART (Universal Asynchronous Receiver/Transmitter) abstraction
//!
//! For new code implement [`embedded_io::Read`] and [`embedded_io::Write`] on
//! your UART type — `embedded-hal 1.0` dropped the old `serial` module in
//! favour of the `embedded-io` crate.
//!
//! Platform-specific configuration (baud rate, parity, etc.) is not covered
//! by `embedded-io`; continue to use the [`UartConfig`] struct and the
//! platform's own `configure()` inherent method.
//!
//! Extension methods such as [`UartPort::available`] and the interrupt-enable
//! API in [`UartPortAsync`] have no `embedded-io` equivalent and are kept as
//! qp-rs extension traits.

use crate::error::HalResult;

// Re-export embedded-io traits for convenience
pub use embedded_io::{ErrorType as IoErrorType, Read as IoRead, Write as IoWrite};

/// UART data bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    Eight,
}

/// UART stop bits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBits {
    One,
    Two,
}

/// UART parity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    None,
    Even,
    Odd,
}

/// UART flow control
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    None,
    RtsCts,
}

/// UART configuration (used by platform `configure()` extension methods)
#[derive(Debug, Clone)]
pub struct UartConfig {
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub flow_control: FlowControl,
}

impl Default for UartConfig {
    fn default() -> Self {
        Self {
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            stop_bits: StopBits::One,
            parity: Parity::None,
            flow_control: FlowControl::None,
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy traits — DEPRECATED; implement embedded_io::Read + Write instead
// ---------------------------------------------------------------------------

/// UART peripheral trait
///
/// # Deprecated
/// Implement [`embedded_io::Read`] + [`embedded_io::Write`] instead.
#[deprecated(since = "0.2.0", note = "implement embedded_io::Read + embedded_io::Write instead")]
pub trait UartPort: Send + Sync {
    /// Configure UART parameters
    fn configure(&mut self, config: &UartConfig) -> HalResult<()>;

    /// Write data (blocking)
    fn write(&mut self, data: &[u8]) -> HalResult<usize>;

    /// Read data (blocking with timeout in milliseconds)
    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> HalResult<usize>;

    /// Bytes available in RX buffer
    fn available(&self) -> usize;

    /// Flush TX buffer
    fn flush(&mut self) -> HalResult<()>;
}

/// UART with interrupt/DMA support
///
/// # Deprecated
/// Use platform-specific IRQ configuration together with `embedded_io::Read`.
#[deprecated(since = "0.2.0", note = "use platform-specific IRQ configuration instead")]
pub trait UartPortAsync: UartPort {
    /// Enable RX interrupt
    fn enable_rx_interrupt(&mut self) -> HalResult<()>;

    /// Disable RX interrupt
    fn disable_rx_interrupt(&mut self) -> HalResult<()>;

    /// Enable TX complete interrupt
    fn enable_tx_interrupt(&mut self) -> HalResult<()>;

    /// Disable TX complete interrupt
    fn disable_tx_interrupt(&mut self) -> HalResult<()>;
}
