//! Null RF PHY for POSIX host testing.
//!
//! `NullRf` implements [`RfPhy`] without real hardware. It prints each
//! transmission to stdout and then reports a synthetic [`PhyEvent::TxDone`] on
//! the next `poll_irq`, so the full app → comms → `RfStackAO` TX-completion path
//! (including `RF_TX_DONE_SIG`) can be exercised on the host by draining the PHY
//! with [`RfStackAO::pump`](crate::stack::RfStackAO::pump).

use hal::rf::{RfPhy, RadioMode, RfTxConfig, RfRxConfig, RxMetadata, PhyEvent};
use hal::HalError;

/// No-op RF PHY that logs transmitted frames and synthesises a `TxDone`.
#[derive(Default)]
pub struct NullRf {
    /// Set by `transmit`, drained by `poll_irq` as a synthetic `TxDone`.
    tx_done_pending: bool,
}

impl NullRf {
    /// Create a new host null PHY.
    pub fn new() -> Self { Self::default() }
}

impl RfPhy for NullRf {
    fn init(&mut self) -> Result<(), HalError> { Ok(()) }
    fn set_mode(&mut self, _mode: RadioMode) -> Result<(), HalError> { Ok(()) }
    fn configure_tx(&mut self, _cfg: &RfTxConfig) -> Result<(), HalError> { Ok(()) }
    fn configure_rx(&mut self, _cfg: &RfRxConfig) -> Result<(), HalError> { Ok(()) }

    fn transmit(&mut self, payload: &[u8]) -> Result<(), HalError> {
        cprint!("NullRf TX: ");
        for b in payload { cprint!("{b:02x} "); }
        cprintln!();
        self.tx_done_pending = true;
        Ok(())
    }

    fn read_rx(&mut self, _buf: &mut [u8], _meta: &RxMetadata) -> Result<(), HalError> { Ok(()) }

    fn poll_irq(&mut self) -> Result<Option<PhyEvent>, HalError> {
        if self.tx_done_pending {
            self.tx_done_pending = false;
            Ok(Some(PhyEvent::TxDone))
        } else {
            Ok(None)
        }
    }

    fn clear_irq(&mut self) -> Result<(), HalError> { Ok(()) }
    fn rssi(&mut self) -> Result<i16, HalError> { Ok(-50) }
    fn chip_name(&self) -> &'static str { "NullRf" }
}
