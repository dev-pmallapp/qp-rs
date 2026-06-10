//! QS-RX: host-to-target command parser.
//!
//! The host tool sends HDLC-framed command packets to the target.  This module
//! provides an incremental byte-at-a-time parser (`RxParser`) that decodes those
//! frames and returns strongly-typed `RxCmd` values.
//!
//! Frame format (mirrors QS-TX direction):
//!   `FLAG(0x7E) | SEQ | CMD_TYPE | [PAYLOAD…] | CHECKSUM | FLAG`
//!
//! Byte stuffing: `0x7E` and `0x7D` are escaped as `0x7D, byte ^ 0x20`.

/// Command types sent by the host tool (QP/C++ `QS_RX` command IDs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RxCmd {
    /// Query target info (triggers `TARGET_INFO` response).
    Info,
    /// Soft-reset request.
    Reset,
    /// Apply a global filter bitmask (128 bits = 16 bytes, little-endian).
    GlbFilter { bits: [u8; 16] },
    /// Apply a local (per-object) filter.
    LocFilter { kind: u8, obj_ptr: u64 },
    /// Apply an AO filter (allow/block records for one AO by priority).
    AoFilter { prio: u8 },
    /// Advance the tick clock by `rate` count.
    Tick { rate: u8 },
    /// Unrecognised command; raw bytes preserved.
    Unknown { cmd: u8, payload: Vec<u8> },
}

/// QS-RX command type constants (matching QP/C++ `QS_RX` IDs).
pub mod cmd {
    pub const INFO:       u8 = 0x01;
    pub const RESET:      u8 = 0x02;
    pub const GLB_FILTER: u8 = 0x06;
    pub const LOC_FILTER: u8 = 0x07;
    pub const AO_FILTER:  u8 = 0x08;
    pub const TICK:       u8 = 0x0A;
}

/// Incremental HDLC frame decoder for QS-RX.
///
/// Feed bytes one at a time with [`push`].  Complete, checksum-verified frames
/// are returned as `Some(RxCmd)`.
pub struct RxParser {
    state:    RxState,
    buf:      Vec<u8>,
    checksum: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RxState {
    Idle,
    InFrame,
    Escaped,
}

const FLAG: u8 = 0x7E;
const ESC:  u8 = 0x7D;
const ESC_XOR: u8 = 0x20;

impl RxParser {
    pub fn new() -> Self {
        Self {
            state:    RxState::Idle,
            buf:      Vec::with_capacity(32),
            checksum: 0,
        }
    }

    /// Feed one byte.  Returns a decoded command if the byte completed a valid frame.
    pub fn push(&mut self, byte: u8) -> Option<RxCmd> {
        match self.state {
            RxState::Idle => {
                if byte == FLAG {
                    self.buf.clear();
                    self.checksum = 0;
                    self.state = RxState::InFrame;
                }
                None
            }
            RxState::InFrame => {
                if byte == FLAG {
                    // End of frame
                    let result = self.try_decode();
                    self.buf.clear();
                    self.checksum = 0;
                    self.state = RxState::Idle;
                    result
                } else if byte == ESC {
                    self.state = RxState::Escaped;
                    None
                } else {
                    self.accept(byte);
                    None
                }
            }
            RxState::Escaped => {
                self.state = RxState::InFrame;
                self.accept(byte ^ ESC_XOR);
                None
            }
        }
    }

    /// Feed a slice of bytes; returns all decoded commands in order.
    pub fn push_slice(&mut self, bytes: &[u8]) -> Vec<RxCmd> {
        bytes.iter().filter_map(|&b| self.push(b)).collect()
    }

    fn accept(&mut self, byte: u8) {
        self.checksum = self.checksum.wrapping_add(byte);
        self.buf.push(byte);
    }

    fn try_decode(&mut self) -> Option<RxCmd> {
        // Minimum frame: SEQ(1) + CMD(1) + CHECKSUM(1) = 3 bytes
        if self.buf.len() < 3 {
            return None;
        }

        // Last byte is the checksum complement; validate.
        // checksum covers all bytes including the complement itself
        // QP/C++ convention: sum of all bytes (including checksum byte) == 0xFF
        let total: u8 = self.buf.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0xFF {
            return None;
        }

        // buf = [seq, cmd_type, payload..., checksum]
        let cmd_type = self.buf[1];
        let payload  = &self.buf[2..self.buf.len() - 1];

        Some(Self::decode_cmd(cmd_type, payload))
    }

    fn decode_cmd(cmd_type: u8, payload: &[u8]) -> RxCmd {
        match cmd_type {
            cmd::INFO  => RxCmd::Info,
            cmd::RESET => RxCmd::Reset,
            cmd::GLB_FILTER if payload.len() >= 16 => {
                let mut bits = [0u8; 16];
                bits.copy_from_slice(&payload[..16]);
                RxCmd::GlbFilter { bits }
            }
            cmd::LOC_FILTER if payload.len() >= 9 => {
                let kind    = payload[0];
                let obj_ptr = u64::from_le_bytes(payload[1..9].try_into().unwrap());
                RxCmd::LocFilter { kind, obj_ptr }
            }
            cmd::AO_FILTER if !payload.is_empty() => {
                RxCmd::AoFilter { prio: payload[0] }
            }
            cmd::TICK if !payload.is_empty() => {
                RxCmd::Tick { rate: payload[0] }
            }
            _ => RxCmd::Unknown {
                cmd: cmd_type,
                payload: payload.to_vec(),
            },
        }
    }
}

impl Default for RxParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_frame(seq: u8, cmd: u8, payload: &[u8]) -> Vec<u8> {
        let mut raw: Vec<u8> = Vec::new();
        raw.push(seq);
        raw.push(cmd);
        raw.extend_from_slice(payload);
        let sum: u8 = raw.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        raw.push(!sum); // checksum complement so that total == 0xFF

        let mut frame = vec![FLAG];
        for &byte in &raw {
            if byte == FLAG || byte == ESC {
                frame.push(ESC);
                frame.push(byte ^ ESC_XOR);
            } else {
                frame.push(byte);
            }
        }
        frame.push(FLAG);
        frame
    }

    #[test]
    fn decode_info_command() {
        let frame = encode_frame(1, cmd::INFO, &[]);
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Info]);
    }

    #[test]
    fn decode_reset_command() {
        let frame = encode_frame(2, cmd::RESET, &[]);
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Reset]);
    }

    #[test]
    fn decode_tick_command() {
        let frame = encode_frame(3, cmd::TICK, &[1]);
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Tick { rate: 1 }]);
    }

    #[test]
    fn decode_ao_filter() {
        let frame = encode_frame(4, cmd::AO_FILTER, &[7]);
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::AoFilter { prio: 7 }]);
    }

    #[test]
    fn decode_glb_filter() {
        let bits = [0xFFu8; 16];
        let frame = encode_frame(5, cmd::GLB_FILTER, &bits);
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::GlbFilter { bits }]);
    }

    #[test]
    fn bad_checksum_discarded() {
        let mut frame = encode_frame(1, cmd::INFO, &[]);
        // Corrupt the checksum byte (second-to-last byte before trailing FLAG)
        let last = frame.len() - 2;
        frame[last] ^= 0x01;
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&frame);
        assert!(cmds.is_empty());
    }

    #[test]
    fn two_frames_back_to_back() {
        let mut data = encode_frame(1, cmd::INFO, &[]);
        data.extend(encode_frame(2, cmd::RESET, &[]));
        let mut parser = RxParser::new();
        let cmds = parser.push_slice(&data);
        assert_eq!(cmds, vec![RxCmd::Info, RxCmd::Reset]);
    }
}
