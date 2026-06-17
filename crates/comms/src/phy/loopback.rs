//! Loopback PHY driver for host-based testing.

use std::collections::VecDeque;
use hal::rf::{RfPhy, RadioMode, RfTxConfig, RfRxConfig, RxMetadata, PhyEvent};
use hal::error::HalResult;

pub struct LoopbackPhy {
    rx_queue: VecDeque<(Vec<u8>, RxMetadata)>,
    pending_rx: Option<Vec<u8>>,
}

impl LoopbackPhy {
    pub fn new() -> Self {
        Self {
            rx_queue: VecDeque::new(),
            pending_rx: None,
        }
    }

    /// Inject a raw frame as if received over the air.
    pub fn inject(&mut self, bytes: &[u8]) {
        self.rx_queue.push_back((bytes.to_vec(), RxMetadata::default()));
    }
}

impl RfPhy for LoopbackPhy {
    fn init(&mut self) -> HalResult<()> { Ok(()) }
    fn set_mode(&mut self, _mode: RadioMode) -> HalResult<()> { Ok(()) }
    fn configure_tx(&mut self, _cfg: &RfTxConfig) -> HalResult<()> { Ok(()) }
    fn configure_rx(&mut self, _cfg: &RfRxConfig) -> HalResult<()> { Ok(()) }

    fn transmit(&mut self, payload: &[u8]) -> HalResult<()> {
        // Echo back (loopback): inject what was transmitted as an RX frame
        self.rx_queue.push_back((payload.to_vec(), RxMetadata::default()));
        Ok(())
    }

    fn read_rx(&mut self, buf: &mut [u8], _meta: &RxMetadata) -> HalResult<()> {
        if let Some(bytes) = self.pending_rx.take() {
            let len = bytes.len().min(buf.len());
            buf[..len].copy_from_slice(&bytes[..len]);
        }
        Ok(())
    }

    fn poll_irq(&mut self) -> HalResult<Option<PhyEvent>> {
        if let Some((bytes, meta)) = self.rx_queue.pop_front() {
            let mut m = meta;
            m.pkt_len = bytes.len() as u8;
            self.pending_rx = Some(bytes);
            Ok(Some(PhyEvent::RxDone(m)))
        } else {
            Ok(None)
        }
    }

    fn clear_irq(&mut self) -> HalResult<()> { Ok(()) }
    fn rssi(&mut self) -> HalResult<i16> { Ok(-50) }
    fn chip_name(&self) -> &'static str { "Loopback" }
}
