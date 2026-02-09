//! UART (Universal Asynchronous Receiver/Transmitter) abstraction

use crate::error::HalResult;

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

/// UART configuration
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

/// UART peripheral trait
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
