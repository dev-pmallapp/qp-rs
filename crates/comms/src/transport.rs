//! Transport layer implementations.

use crate::buf::Frame;
use crate::error::CommsError;
use crate::stack::Layer;

pub struct ReliableTransport {
    seq:          u8,   // next TX sequence number
    acked:        u8,   // last in-sequence SEQ received from peer
    retries:      u8,   // retransmit attempts remaining
    max_retries:  u8,   // configurable (default 3)
    state:        TransportState,
}

/// Reliable-transport state, exposed via [`ReliableTransport::state`] so the
/// owning active object can drive its own state machine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportState {
    /// No transfer in progress.
    Idle,
    /// A PDU was sent and we are awaiting its ACK.
    WaitingAck,
}

/// Action returned to `RfStackAO` after a transport event.
///
/// The retransmit frame is NOT carried here — it is owned by `RfStackAO`
/// because the frame must be saved *after* MAC encapsulation (post-MAC bytes).
/// Returning a plain `ShouldRetransmit` signal keeps `ReliableTransport`
/// frame-free and `no_alloc`.
pub enum TransportAction {
    /// No action needed.
    Nothing,
    /// Peer ACKed the last PDU — TX session complete.
    TxComplete,
    /// Retransmit the saved frame (owned by the caller/AO).
    ShouldRetransmit,
    /// Retransmit limit exhausted — notify application of failure.
    GiveUp,
}

impl ReliableTransport {
    pub fn new(max_retries: u8) -> Self {
        Self {
            seq: 0,
            acked: 0,
            retries: 0,
            max_retries,
            state: TransportState::Idle,
        }
    }

    /// Returns the current transport state (for AO state machine transitions).
    pub fn state(&self) -> TransportState { self.state }

    /// Called by `RfStackAO` when its retransmit `TimeEvent` fires.
    ///
    /// The AO must hold a copy of the post-MAC frame and replay it to the PHY
    /// when this returns `ShouldRetransmit`.
    pub fn on_timeout(&mut self) -> TransportAction {
        if self.state == TransportState::WaitingAck {
            if self.retries > 0 {
                self.retries -= 1;
                TransportAction::ShouldRetransmit
            } else {
                self.state = TransportState::Idle;
                TransportAction::GiveUp
            }
        } else {
            TransportAction::Nothing
        }
    }

    /// Called when a pure-ACK PDU arrives from the peer.
    ///
    /// `ack_seq` is the SEQ number from the ACK PDU header.
    pub fn on_ack_received(&mut self, ack_seq: u8) -> TransportAction {
        if ack_seq == self.seq.wrapping_sub(1) {
            self.state = TransportState::Idle;
            TransportAction::TxComplete
        } else {
            TransportAction::Nothing
        }
    }
}

impl Layer for ReliableTransport {
    /// Egress: prepend 5-byte transport header and arm retransmit tracking.
    ///
    /// The caller (`RfStackAO`) must snapshot the *fully-MAC-encoded* frame
    /// after the entire stack `.down()` sequence completes, so the retransmit
    /// can replay complete PHY bytes without re-running the MAC.
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let payload_len = frame.len();
        let hdr = frame.prepend_header(5)?;
        hdr[0] = self.seq;
        hdr[1] = self.acked;
        hdr[2] = TransportFlags::FIRST_FRAG
               | TransportFlags::LAST_FRAG
               | TransportFlags::ACK_REQ;
        hdr[3] = payload_len as u8;
        hdr[4] = 0; // LENHI — always 0 for LoRaWAN payloads ≤ 242 bytes

        self.retries = self.max_retries;
        self.state   = TransportState::WaitingAck;
        self.seq     = self.seq.wrapping_add(1);
        Ok(())
    }

    /// Ingress: validate and strip the 5-byte transport header.
    ///
    /// Returns `Ok(false)` for duplicates and pure-ACK PDUs (no payload
    /// to deliver to the application).
    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 5 { return Ok(false); }
        let raw = frame.strip_header(5)?;
        let seq   = raw[0];
        let flags = raw[2];

        if flags & TransportFlags::IS_ACK != 0 {
            // Pure ACK PDU — handle, but pass nothing to application
            self.on_ack_received(seq);
            return Ok(false);
        }

        // Duplicate detection: already ACKed this SEQ
        if seq == self.acked && self.state != TransportState::Idle {
            return Ok(false);
        }

        self.acked = seq;
        Ok(true)
    }

    fn on_timeout(&mut self) -> TransportAction {
        self.on_timeout()
    }
}

/// Flag constants for the transport header FLAGS byte.
#[allow(non_snake_case)]
pub mod TransportFlags {
    pub const FIRST_FRAG: u8 = 0x01;
    pub const LAST_FRAG:  u8 = 0x02;
    pub const ACK_REQ:    u8 = 0x04;
    pub const IS_ACK:     u8 = 0x08;
    pub const IS_NACK:    u8 = 0x10;
    pub const RESET:      u8 = 0x20;
}

pub struct UnreliableTransport {
    seq: u8,
}

impl Default for UnreliableTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UnreliableTransport {
    pub const fn new() -> Self { Self { seq: 0 } }
}

impl Layer for UnreliableTransport {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let len = frame.len() as u16;
        let hdr = frame.prepend_header(5)?;
        hdr[0] = self.seq;
        hdr[1] = 0;
        hdr[2] = TransportFlags::FIRST_FRAG | TransportFlags::LAST_FRAG;
        hdr[3] = len as u8;
        hdr[4] = (len >> 8) as u8;
        self.seq = self.seq.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 5 { return Ok(false); }
        frame.strip_header(5)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buf::Frame;

    // ── ReliableTransport ────────────────────────────────────────────────────

    #[test]
    fn reliable_down_sets_waiting_ack_state() {
        let mut t = ReliableTransport::new(3);
        let mut f = Frame::new();
        f.write_payload(b"hello world").unwrap();

        t.down(&mut f).unwrap();

        assert_eq!(t.state, TransportState::WaitingAck);
        assert_eq!(t.seq, 1); // incremented
        assert_eq!(t.retries, 3);

        // Header must be 5 bytes prepended
        // payload was 11 bytes → after down: 5 + 11 = 16 bytes
        assert_eq!(f.len(), 16);
        let payload = f.payload();
        assert_eq!(payload[0], 0); // SEQ = 0
        assert_eq!(payload[2], TransportFlags::FIRST_FRAG | TransportFlags::LAST_FRAG | TransportFlags::ACK_REQ);
        assert_eq!(payload[3], 11); // LEN = 11
    }

    #[test]
    fn reliable_on_timeout_returns_should_retransmit_then_give_up() {
        let mut t = ReliableTransport::new(2);
        let mut f = Frame::new();
        f.write_payload(b"x").unwrap();
        t.down(&mut f).unwrap(); // state = WaitingAck, retries = 2

        // First timeout → retry 1
        assert!(matches!(t.on_timeout(), TransportAction::ShouldRetransmit));
        assert_eq!(t.retries, 1);

        // Second timeout → retry 2
        assert!(matches!(t.on_timeout(), TransportAction::ShouldRetransmit));
        assert_eq!(t.retries, 0);

        // Third timeout → exhausted
        assert!(matches!(t.on_timeout(), TransportAction::GiveUp));
        assert_eq!(t.state, TransportState::Idle);
    }

    #[test]
    fn reliable_ack_received_clears_state() {
        let mut t = ReliableTransport::new(3);
        let mut f = Frame::new();
        f.write_payload(b"hello").unwrap();
        t.down(&mut f).unwrap(); // seq was 0, now seq=1, WaitingAck

        // ACK for SEQ 0 (seq.wrapping_sub(1) = 0)
        let action = t.on_ack_received(0);
        assert!(matches!(action, TransportAction::TxComplete));
        assert_eq!(t.state, TransportState::Idle);
    }

    #[test]
    fn reliable_stale_ack_ignored() {
        let mut t = ReliableTransport::new(3);
        let mut f = Frame::new();
        f.write_payload(b"hi").unwrap();
        t.down(&mut f).unwrap(); // seq=0 → seq=1, WaitingAck

        // ACK for wrong SEQ
        let action = t.on_ack_received(5);
        assert!(matches!(action, TransportAction::Nothing));
        assert_eq!(t.state, TransportState::WaitingAck);
    }

    #[test]
    fn reliable_up_strips_header_and_passes_payload() {
        let mut t = ReliableTransport::new(3);
        // Build header: SEQ=5, ACK=0, FLAGS=FIRST_FRAG|LAST_FRAG|ACK_REQ, LEN=4
        let hdr_payload: &[u8] = &[5, 0, 0x07, 4, 0, b't', b'e', b's', b't'];
        let mut raw = Frame::new();
        raw.set_received_len(hdr_payload.len());
        raw.raw_buf_for_dma()[..hdr_payload.len()].copy_from_slice(hdr_payload);

        let keep = t.up(&mut raw).unwrap();
        assert!(keep);
        assert_eq!(raw.payload(), b"test");
        assert_eq!(t.acked, 5);
    }

    #[test]
    fn reliable_up_rejects_pure_ack() {
        let mut t = ReliableTransport::new(3);
        let mut f = Frame::new();
        f.write_payload(b"data").unwrap();
        t.down(&mut f).unwrap(); // seq=0 sent, state=WaitingAck

        // Construct a pure ACK PDU: SEQ=0, IS_ACK flag
        let ack_bytes: &[u8] = &[0, 0, TransportFlags::IS_ACK, 0, 0];
        let mut ack_frame = Frame::new();
        ack_frame.set_received_len(ack_bytes.len());
        ack_frame.raw_buf_for_dma()[..5].copy_from_slice(ack_bytes);

        let keep = t.up(&mut ack_frame).unwrap();
        assert!(!keep); // pure ACK: no payload to app
        assert_eq!(t.state, TransportState::Idle);
    }

    // ── UnreliableTransport ──────────────────────────────────────────────────

    #[test]
    fn unreliable_down_up_round_trip() {
        let mut tx = UnreliableTransport::new();
        let mut rx = UnreliableTransport::new();

        let payload = b"datagram payload";
        let mut f = Frame::new();
        f.write_payload(payload).unwrap();

        tx.down(&mut f).unwrap();
        // After down: 5-byte header + 16-byte payload = 21 bytes
        assert_eq!(f.len(), 5 + payload.len());

        rx.up(&mut f).unwrap();
        // After up: header stripped, payload recovered
        assert_eq!(f.payload(), payload);
    }

    #[test]
    fn unreliable_seq_increments() {
        let mut t = UnreliableTransport::new();
        for expected_seq in 0u8..4 {
            let mut f = Frame::new();
            f.write_payload(b"x").unwrap();
            t.down(&mut f).unwrap();
            // SEQ is first byte of the 5-byte header (now at start of frame)
            assert_eq!(f.payload()[0], expected_seq);
        }
    }
}
