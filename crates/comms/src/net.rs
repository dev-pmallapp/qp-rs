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

/// Broadcast destination address — accepted by every node's `up()` filter.
pub const BROADCAST_ADDR: u16 = 0xFFFF;

pub struct Network {
    bindings: [Option<PortBinding>; MAX_PORT_BINDINGS],
    /// This node's own address (src on TX, filter target on RX).
    address: u16,
    /// Destination address queued by `set_tx_meta` for the *next* `down()`.
    pending_dst: u16,
    /// Kind/proto byte queued by `set_tx_meta` for the *next* `down()`.
    pending_kind: u8,
    /// Source address extracted by the most recent successful `up()`.
    last_rx_src: u16,
    /// Kind/proto byte extracted by the most recent successful `up()`.
    last_rx_kind: u8,
}

impl Network {
    /// Create a `Network` layer addressed as `address` on this node.
    pub const fn new(address: u16) -> Self {
        Self {
            bindings: [const { None }; MAX_PORT_BINDINGS],
            address,
            pending_dst: BROADCAST_ADDR,
            pending_kind: 0,
            last_rx_src: 0,
            last_rx_kind: 0,
        }
    }

    /// This node's configured address.
    pub const fn address(&self) -> u16 {
        self.address
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
        // Prepend a 5-byte network header: [src:u16 LE][dst:u16 LE][kind:u8].
        // Like every other layer, we only move the frame's `start` cursor down
        // (via `prepend_header`); the headroom is preserved so the MAC layer can
        // still prepend its own header afterwards. `phy_bytes()` already returns
        // `buf[start..end]`, so no byte copy is needed.
        let dst = self.pending_dst;
        let kind = self.pending_kind;
        // Reset to broadcast/0 so a caller that forgets `set_tx_meta` before a
        // later, unrelated TX doesn't silently reuse a stale unicast target.
        self.pending_dst = BROADCAST_ADDR;
        self.pending_kind = 0;

        let hdr = frame.prepend_header(5)?;
        hdr[0..2].copy_from_slice(&self.address.to_le_bytes());
        hdr[2..4].copy_from_slice(&dst.to_le_bytes());
        hdr[4] = kind;
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        // Verify header and strip it. If the destination address does not match
        // this node (or is not broadcast), drop the frame.
        if frame.len() < 5 {
            return Ok(false);
        }
        let hdr = frame.strip_header(5)?;
        let src = u16::from_le_bytes([hdr[0], hdr[1]]);
        let dst = u16::from_le_bytes([hdr[2], hdr[3]]);
        let kind = hdr[4];
        // Accept if dst is our address or broadcast.
        if dst != self.address && dst != BROADCAST_ADDR {
            return Ok(false);
        }
        self.last_rx_src = src;
        self.last_rx_kind = kind;
        Ok(true)
    }

    fn set_tx_meta(&mut self, dst: u16, kind: u8) {
        self.pending_dst = dst;
        self.pending_kind = kind;
    }

    fn last_rx_meta(&self) -> (u16, u8) {
        (self.last_rx_src, self.last_rx_kind)
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
        let mut net = Network::new(0x0001);
        let mut f = Frame::new();
        f.write_payload(b"payload").unwrap();
        net.down(&mut f).unwrap();
        // Header added (5 bytes) + payload; ensure length is correct.
        assert_eq!(f.len(), 5 + 7);

        let mut recv = loopback(&f);
        let ok = net.up(&mut recv).unwrap();
        assert!(ok);
        assert_eq!(recv.payload(), b"payload");
    }

    #[test]
    fn network_composes_with_mac_headroom() {
        // Regression: `down()` must preserve headroom so a MAC layer can still
        // prepend after the network header (transport → network → mac order).
        let mut net = Network::new(0x0001);
        let mut f = Frame::new();
        f.write_payload(b"payload").unwrap();
        net.down(&mut f).unwrap();
        // A subsequent 9-byte MAC header must still fit in the headroom.
        assert!(f.prepend_header(9).is_ok(), "network must leave MAC headroom");
    }

    #[test]
    fn network_drops_wrong_dst() {
        let mut net = Network::new(0x0010);
        // Build a frame addressed to a different, non-broadcast destination.
        let mut f = Frame::new();
        f.write_payload(b"x").unwrap();
        let hdr = f.prepend_header(5).unwrap();
        hdr[0..2].copy_from_slice(&0x0020u16.to_le_bytes()); // src
        hdr[2..4].copy_from_slice(&0x0030u16.to_le_bytes()); // dst (different)
        hdr[4] = 0;

        let mut recv = loopback(&f);
        let ok = net.up(&mut recv).unwrap();
        assert!(!ok);
    }

    #[test]
    fn network_tx_meta_roundtrips_dst_and_kind() {
        // set_tx_meta must be consumed by the next down() and reset to
        // broadcast/0 afterwards, and up() must expose the peer's src/kind
        // via last_rx_meta().
        let mut tx = Network::new(0x0001);
        tx.set_tx_meta(0x0002, 7);
        let mut f = Frame::new();
        f.write_payload(b"hi").unwrap();
        tx.down(&mut f).unwrap();

        let mut rx = Network::new(0x0002);
        let mut recv = loopback(&f);
        assert!(rx.up(&mut recv).unwrap());
        assert_eq!(rx.last_rx_meta(), (0x0001, 7));

        // A second, meta-less down() on tx must fall back to broadcast/0.
        let mut f2 = Frame::new();
        f2.write_payload(b"bye").unwrap();
        tx.down(&mut f2).unwrap();
        let hdr = &f2.phy_bytes()[0..5];
        assert_eq!(u16::from_le_bytes([hdr[2], hdr[3]]), BROADCAST_ADDR);
        assert_eq!(hdr[4], 0);
    }
}
