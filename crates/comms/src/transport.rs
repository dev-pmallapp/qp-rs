//! Transport layer implementations.

use crate::buf::Frame;
use crate::error::CommsError;
use crate::stack::Layer;

pub struct ReliableTransport {
    seq:          u8,                  // next TX sequence number
    acked:        u8,                  // last ACKed SEQ from peer
    retransmit:   Option<Frame>,       // copy of last unACKed frame
    retries:      u8,                  // retransmit attempts remaining
    max_retries:  u8,                  // configurable (default 3)
    state:        TransportState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransportState {
    Idle,
    WaitingAck,
    Receiving { total: u8, seen_mask: u8 },
}

pub enum TransportAction {
    Nothing,
    TxComplete,
    Retransmit(Frame),
    GiveUp,
}

impl ReliableTransport {
    pub fn new(max_retries: u8) -> Self {
        Self {
            seq: 0, acked: 0,
            retransmit: None,
            retries: 0, max_retries,
            state: TransportState::Idle,
        }
    }

    /// Called by `RfStackAO` when its retransmit `TimeEvent` fires.
    pub fn on_timeout(&mut self) -> TransportAction {
        if let Some(ref frame) = self.retransmit {
            if self.retries > 0 {
                self.retries -= 1;
                TransportAction::Retransmit(frame.clone())
            } else {
                self.state = TransportState::Idle;
                self.retransmit = None;
                TransportAction::GiveUp
            }
        } else {
            TransportAction::Nothing
        }
    }

    pub fn on_ack_received(&mut self, ack_seq: u8) -> TransportAction {
        if ack_seq == self.seq.wrapping_sub(1) {
            self.retransmit = None;
            self.state = TransportState::Idle;
            TransportAction::TxComplete
        } else {
            TransportAction::Nothing
        }
    }
}

impl Layer for ReliableTransport {
    fn down(&mut self, frame: &mut Frame) -> Result<(), CommsError> {
        let payload_len = frame.len();
        let hdr = frame.prepend_header(5)?;
        hdr[0] = self.seq;
        hdr[1] = self.acked;
        hdr[2] = TransportFlags::FIRST_FRAG
               | TransportFlags::LAST_FRAG
               | TransportFlags::ACK_REQ;
        hdr[3] = payload_len as u8;
        hdr[4] = 0;

        // Save for potential retransmit (clone the frame state)
        let mut save = Frame::new();
        save.write_payload(frame.payload())?;
        self.retransmit = Some(save);
        self.retries     = self.max_retries;
        self.state       = TransportState::WaitingAck;
        self.seq         = self.seq.wrapping_add(1);
        Ok(())
    }

    fn up(&mut self, frame: &mut Frame) -> Result<bool, CommsError> {
        if frame.len() < 5 { return Ok(false); }
        let raw = frame.strip_header(5)?;
        let hdr = [raw[0], raw[1], raw[2], raw[3], raw[4]];
        let seq     = hdr[0];
        let flags   = hdr[2];

        // Duplicate detection: discard if already seen
        if seq == self.acked { return Ok(false); }

        if flags & TransportFlags::IS_ACK != 0 {
            // Pure ACK — no payload to pass up
            self.on_ack_received(seq);
            return Ok(false);
        }

        self.acked = seq;
        Ok(true)
    }
}

/// Flag constants for the transport header FLAGS byte.
pub mod TransportFlags {
    pub const FIRST_FRAG: u8 = 0x01;
    pub const LAST_FRAG:  u8 = 0x02;
    pub const ACK_REQ:    u8 = 0x04;
    pub const IS_ACK:     u8 = 0x08;
    pub const IS_NACK:    u8 = 0x10;
    pub const RESET:      u8 = 0x20;
}

pub struct UnreliableTransport {
    seq: u8
}

impl UnreliableTransport {
    pub const fn new() -> Self {
        Self { seq: 0 }
    }
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
