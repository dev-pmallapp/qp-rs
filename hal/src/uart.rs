//! UART (Universal Asynchronous Receiver/Transmitter) abstraction
//!
//! For new code implement [`embedded_io::Read`] and [`embedded_io::Write`] on
//! your UART type — `embedded-hal 1.0` dropped the old `serial` module in
//! favour of the `embedded-io` crate.
//!
//! Platform-specific configuration (baud rate, parity, etc.) is not covered
//! by `embedded-io`; continue to use the [`UartConfig`] struct and the
//! platform's own `configure()` inherent method. RX-available queries and
//! interrupt-enable APIs likewise have no `embedded-io` equivalent and are
//! exposed as platform inherent methods.

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
