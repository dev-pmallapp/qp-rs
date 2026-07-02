//! Network routing and dispatch layer.

use crate::error::CommsError;
use crate::stack::Layer;
use crate::buf::Frame;
use qf::event::Signal;

/// Maximum port → signal bindings in the dispatch table.
const MAX_PORT_BINDINGS: usize = 8;

/// Maps a LoRaWAN FPort (or generic "service identifier") to a QF signal.
pub struct PortBinding {
    pub port:   u8,
    pub signal: Signal,
}

pub struct Network {
    bindings: [Option<PortBinding>; MAX_PORT_BINDINGS],
    /// Simple device address for this node
    address: u8,
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

impl Network {
    pub const fn new() -> Self {
        Self { bindings: [const { None }; MAX_PORT_BINDINGS], address: 0x01 }
    }

    /// Register a port → signal mapping. Returns `Err` if the table is full.
    pub fn bind(&mut self, port: u8, signal: Signal) -> Result<(), CommsError> {
        for slot in &mut self.bindings {
            if slot.is_none() {
                *slot = Some(PortBinding { port, signal });
                return Ok(());
            }
        }
        Err(CommsError::TableFull)
    }

    /// Resolve port to signal for application dispatch.
    pub fn resolve(&self, port: u8) -> Option<Signal> {
        self.bindings.iter()
            .find_map(|b| b.as_ref().filter(|b| b.port == port).map(|b| b.signal))
    }
}

impl Layer for Network {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        // Prepend a simple 3-byte network header: [src, dst, proto].
        // Like every other layer, we only move the frame's `start` cursor down
        // (via `prepend_header`); the headroom is preserved so the MAC layer can
        // still prepend its own header afterwards. `phy_bytes()` already returns
        // `buf[start..end]`, so no byte copy is needed.
        let hdr = frame.prepend_header(3)?;
        hdr[0] = self.address; // src
        hdr[1] = 0xFF;         // dst (broadcast)
        hdr[2] = 0;            // protocol / next-layer id
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        // Verify header and strip it. If the destination address does not match
        // this node (or is not broadcast), drop the frame.
        if frame.len() < 3 {
            return Ok(false);
        }
        let hdr = frame.strip_header(3)?;
        let _src = hdr[0];
        let dst = hdr[1];
        // Accept if dst is our address or broadcast (0xFF).
        if dst != self.address && dst != 0xFF {
            return Ok(false);
        }
        // Protocol byte currently unused.
        Ok(true)
    }
}

/// No-op network layer for LoopbackPhy/NullRf tests.
pub struct NoopNetwork;
impl Layer for NoopNetwork {
    fn down(&mut self, _f: &mut Frame) -> Result<(), CommsError> { Ok(()) }
    fn up(&mut self, _f: &mut Frame) -> Result<bool, CommsError> { Ok(true) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::Frame;

    /// Move the on-air bytes of `tx` into a fresh RX frame, mirroring what the
    /// PHY does on receive (`set_received_len` + DMA copy into `buf[0..len]`).
    fn loopback(tx: &Frame) -> Frame {
        let on_air = tx.phy_bytes().to_vec();
        let mut recv = Frame::new();
        recv.set_received_len(on_air.len());
        recv.raw_buf_for_dma()[..on_air.len()].copy_from_slice(&on_air);
        recv
    }

    #[test]
    fn network_roundtrip() {
        let mut net = Network::new();
        let mut f = Frame::new();
        f.write_payload(b"payload").unwrap();
        net.down(&mut f).unwrap();
        // Header added (3 bytes) + payload; ensure length is correct.
        assert_eq!(f.len(), 3 + 7);

        let mut recv = loopback(&f);
        let ok = net.up(&mut recv).unwrap();
        assert!(ok);
        assert_eq!(recv.payload(), b"payload");
    }

    #[test]
    fn network_composes_with_mac_headroom() {
        // Regression: `down()` must preserve headroom so a MAC layer can still
        // prepend after the network header (transport → network → mac order).
        let mut net = Network::new();
        let mut f = Frame::new();
        f.write_payload(b"payload").unwrap();
        net.down(&mut f).unwrap();
        // A subsequent 9-byte MAC header must still fit in the headroom.
        assert!(f.prepend_header(9).is_ok(), "network must leave MAC headroom");
    }

    #[test]
    fn network_drops_wrong_dst() {
        let mut net = Network { address: 0x10, ..Network::new() };
        // Build a frame addressed to a different, non-broadcast destination.
        let mut f = Frame::new();
        f.write_payload(b"x").unwrap();
        let hdr = f.prepend_header(3).unwrap();
        hdr[0] = 0x20; // src
        hdr[1] = 0x30; // dst (different)
        hdr[2] = 0;

        let mut recv = loopback(&f);
        let ok = net.up(&mut recv).unwrap();
        assert!(!ok);
    }
}
