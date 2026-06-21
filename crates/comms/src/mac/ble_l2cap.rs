//! BLE L2CAP MAC layer stub.
//!
//! Proves that `ReliableTransport` and `Network` are reusable across different
//! radio families without any modification — only the MAC and PHY layers differ.
//!
//! # Status
//! This is a compile-only stub. All methods return `Err(CommsError::MacError)`
//! until a real BLE L2CAP implementation is provided.
//!
//! # L2CAP Basic Frame (B-frame) layout
//!
//! ```text
//! ┌──────────┬──────────┬──────────────────┐
//! │ Length   │   CID    │   Information    │
//! │  (2B LE) │ (2B LE)  │   (0..N bytes)   │
//! └──────────┴──────────┴──────────────────┘
//! ```
//!
//! Total header = 4 bytes.  `FRAME_HEADROOM` (32 bytes) easily accommodates this.

use crate::stack::Layer;
use crate::buf::Frame;
use crate::error::CommsError;

/// BLE L2CAP Basic Frame MAC layer.
///
/// Encapsulates the payload in an L2CAP B-frame header (Length + CID).
/// Paired with any `RfPhy` implementation that provides BLE baseband access.
pub struct BleL2capMac {
    /// L2CAP Channel Identifier.
    ///
    /// Fixed channels: 0x0004 = ATT, 0x0005 = L2CAP signalling, 0x0006 = SMP.
    /// Dynamic channels: 0x0040–0x007F (assigned by signalling protocol).
    cid: u16,
}

impl BleL2capMac {
    pub const fn new(cid: u16) -> Self { Self { cid } }

    pub fn cid(&self) -> u16 { self.cid }
}

impl Layer for BleL2capMac {
    /// Egress: prepend 4-byte L2CAP B-frame header.
    ///
    /// Layout: `[Length:2LE][CID:2LE][FRMPayload]`
    ///
    /// # Status
    /// Not yet implemented — returns `Err(CommsError::MacError)`.
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let payload_len = frame.len() as u16;
        let hdr = frame.prepend_header(4)?;
        hdr[0] = payload_len as u8;
        hdr[1] = (payload_len >> 8) as u8;
        hdr[2] = self.cid as u8;
        hdr[3] = (self.cid >> 8) as u8;
        // TODO: BLE PHY integration (connection handle, fragmentation, etc.)
        // For now, header is written but this will not transmit correctly
        // without a real BLE controller driver.
        Err(CommsError::MacError)
    }

    /// Ingress: parse and strip 4-byte L2CAP B-frame header.
    ///
    /// # Status
    /// Not yet implemented — returns `Err(CommsError::MacError)`.
    fn up(&mut self, _frame: &mut Frame) -> Result<bool, CommsError> {
        // TODO: parse Length + CID, validate, strip header.
        Err(CommsError::MacError)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compile-time composition proof
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::NoopNetwork;
    use crate::transport::ReliableTransport;
    use crate::phy::loopback::LoopbackPhy;
    use crate::stack::RfStack;

    /// Prove that `ReliableTransport` and `NoopNetwork` compile with `BleL2capMac`
    /// without any changes — only the MAC layer differs from the LoRa path.
    ///
    /// This is a compile-time (type-system) test; the `_stack` value is never
    /// actually used for transmission because the BLE MAC is not yet implemented.
    #[test]
    fn ble_stack_type_checks() {
        type BleStack = RfStack<ReliableTransport, NoopNetwork, BleL2capMac, LoopbackPhy>;

        // This must compile with the same Transport and Network as the LoRa path.
        let _stack: BleStack = RfStack::new(
            ReliableTransport::new(3),
            NoopNetwork,
            BleL2capMac::new(0x0004), // ATT channel
            LoopbackPhy::new(),
        );

        // Verify field access compiles correctly (no wrong types slipping in).
        assert_eq!(_stack.mac.cid(), 0x0004);
    }
}
