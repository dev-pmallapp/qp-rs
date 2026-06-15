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
//!
//! Command IDs match the `QS_RX*` enum in QP/C++ and the companion QSpy tool
//! in `tools/qspy/src/commands.rs`.

#[cfg(not(feature = "std"))]
extern crate alloc;

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Strongly-typed commands decoded from QS-RX frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RxCmd {
    /// Query target info (triggers `TARGET_INFO` response).
    Info,
    /// Execute a user-defined command; `id` selects the function, `p1`–`p3` are params.
    Command {
        /// User command id selecting the target function.
        id: u8,
        /// First parameter.
        p1: u32,
        /// Second parameter.
        p2: u32,
        /// Third parameter.
        p3: u32,
    },
    /// Soft-reset request.
    Reset,
    /// Advance the tick clock for the given tick rate.
    Tick {
        /// Tick-rate domain to advance.
        rate: u8,
    },
    /// Read memory from the target.
    Peek {
        /// Base address to read from.
        addr: u64,
        /// Byte offset added to `addr`.
        offset: u16,
        /// Size of each element in bytes.
        size: u8,
        /// Number of elements to read.
        num: u8,
    },
    /// Write bytes into target memory.
    Poke {
        /// Base address to write to.
        addr: u64,
        /// Byte offset added to `addr`.
        offset: u16,
        /// Size of each element in bytes.
        size: u8,
        /// Number of elements to write.
        num: u8,
        /// Bytes to write (`size * num` long).
        data: Vec<u8>,
    },
    /// Fill a region of target memory with a repeated pattern.
    Fill {
        /// Base address to fill from.
        addr: u64,
        /// Byte offset added to `addr`.
        offset: u16,
        /// Size of the fill pattern in bytes.
        size: u8,
        /// Number of times to repeat the pattern.
        num: u8,
        /// The fill pattern (`size` bytes).
        data: Vec<u8>,
    },
    /// Start a new QUTest test (clears all registered probes).
    TestSetup,
    /// End the current QUTest test (clears all registered probes).
    TestTeardown,
    /// Resume a QUTest test that paused after emitting `TEST_PAUSED`.
    TestContinue,
    /// Register a test probe: when production code calls `take_test_probe(fn_ptr)`
    /// it will receive `data` (once).
    TestProbe {
        /// Address of the production function the probe is keyed to.
        fn_ptr: u64,
        /// Value delivered to the probe (once).
        data: u32,
    },
    /// Apply a global filter bitmask (128 bits = 16 bytes, little-endian).
    GlbFilter {
        /// 128-bit record-type filter bitmask.
        bits: [u8; 16],
    },
    /// Apply a local (per-object) filter.
    LocFilter {
        /// Object-kind selector.
        kind: u8,
        /// Address of the object to filter.
        obj_ptr: u64,
    },
    /// Apply an AO filter (allow/block records for one AO by priority).
    AoFilter {
        /// Priority of the active object to filter.
        prio: u8,
    },
    /// Set the "current object" (kind + pointer) for query/filter operations.
    CurrObj {
        /// Object-kind selector.
        kind: u8,
        /// Address of the object to make current.
        obj_ptr: u64,
    },
    /// Query the current object's state; `kind` selects the object type.
    QueryCurr {
        /// Object-kind selector.
        kind: u8,
    },
    /// Inject an event directly into an active object identified by `prio`.
    Event {
        /// Priority of the destination active object.
        prio: u8,
        /// Signal of the injected event.
        signal: u16,
        /// Raw event payload bytes.
        payload: Vec<u8>,
    },
    /// Unrecognised command; raw bytes preserved for forward compatibility.
    Unknown {
        /// The unrecognised command type byte.
        cmd: u8,
        /// Raw payload bytes following the command type.
        payload: Vec<u8>,
    },
}

/// QS-RX command type constants — match `QS_RX*` in QP/C++ and
/// `tools/qspy/src/commands.rs`.
pub mod cmd {
    /// Query target info.
    pub const INFO:          u8 = 0;
    /// Execute a user-defined command.
    pub const COMMAND:       u8 = 1;
    /// Soft-reset the target.
    pub const RESET:         u8 = 2;
    /// Advance the tick clock.
    pub const TICK:          u8 = 3;
    /// Read target memory.
    pub const PEEK:          u8 = 4;
    /// Write target memory.
    pub const POKE:          u8 = 5;
    /// Fill target memory with a pattern.
    pub const FILL:          u8 = 6;
    /// Begin a QUTest test.
    pub const TEST_SETUP:    u8 = 7;
    /// End a QUTest test.
    pub const TEST_TEARDOWN: u8 = 8;
    /// Register a QUTest probe.
    pub const TEST_PROBE:    u8 = 9;
    /// Set the global record filter.
    pub const GLB_FILTER:    u8 = 10;
    /// Set a local (per-object) filter.
    pub const LOC_FILTER:    u8 = 11;
    /// Set an active-object filter.
    pub const AO_FILTER:     u8 = 12;
    /// Set the current object.
    pub const CURR_OBJ:      u8 = 13;
    /// Resume a paused QUTest test.
    pub const TEST_CONTINUE: u8 = 14;
    /// Query the current object.
    pub const QUERY_CURR:    u8 = 15;
    /// Inject an event into an active object.
    pub const EVENT:         u8 = 16;
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

const FLAG:    u8 = 0x7E;
const ESC:     u8 = 0x7D;
const ESC_XOR: u8 = 0x20;

impl RxParser {
    /// Creates a parser in the idle state, ready to receive frames.
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
        // QP/C++ convention: sum of all bytes including checksum complement == 0xFF
        let total: u8 = self.buf.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        if total != 0xFF {
            return None;
        }
        // buf = [seq, cmd_type, payload..., checksum_complement]
        let cmd_type = self.buf[1];
        let payload  = &self.buf[2..self.buf.len() - 1];
        Some(Self::decode_cmd(cmd_type, payload))
    }

    fn decode_cmd(cmd_type: u8, payload: &[u8]) -> RxCmd {
        match cmd_type {
            cmd::INFO          => RxCmd::Info,
            cmd::RESET         => RxCmd::Reset,
            cmd::TEST_SETUP    => RxCmd::TestSetup,
            cmd::TEST_TEARDOWN => RxCmd::TestTeardown,
            cmd::TEST_CONTINUE => RxCmd::TestContinue,

            cmd::TICK if !payload.is_empty() =>
                RxCmd::Tick { rate: payload[0] },

            cmd::AO_FILTER if !payload.is_empty() =>
                RxCmd::AoFilter { prio: payload[0] },

            cmd::QUERY_CURR if !payload.is_empty() =>
                RxCmd::QueryCurr { kind: payload[0] },

            cmd::GLB_FILTER if payload.len() >= 16 => {
                let mut bits = [0u8; 16];
                bits.copy_from_slice(&payload[..16]);
                RxCmd::GlbFilter { bits }
            }

            // LOC_FILTER / CURR_OBJ: [kind: 1] [obj_ptr: 8 LE]
            cmd::LOC_FILTER if payload.len() >= 9 => {
                let kind    = payload[0];
                let obj_ptr = u64::from_le_bytes(payload[1..9].try_into().unwrap());
                RxCmd::LocFilter { kind, obj_ptr }
            }
            cmd::CURR_OBJ if payload.len() >= 9 => {
                let kind    = payload[0];
                let obj_ptr = u64::from_le_bytes(payload[1..9].try_into().unwrap());
                RxCmd::CurrObj { kind, obj_ptr }
            }

            // TEST_PROBE: [fn_ptr: 8 LE] [data: 4 LE]  (assumes 64-bit target)
            cmd::TEST_PROBE if payload.len() >= 12 => {
                let fn_ptr = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let data   = u32::from_le_bytes(payload[8..12].try_into().unwrap());
                RxCmd::TestProbe { fn_ptr, data }
            }

            // COMMAND: [id: 1] [p1: 4 LE] [p2: 4 LE] [p3: 4 LE]
            cmd::COMMAND if payload.len() >= 13 => {
                let id = payload[0];
                let p1 = u32::from_le_bytes(payload[1..5].try_into().unwrap());
                let p2 = u32::from_le_bytes(payload[5..9].try_into().unwrap());
                let p3 = u32::from_le_bytes(payload[9..13].try_into().unwrap());
                RxCmd::Command { id, p1, p2, p3 }
            }

            // PEEK: [addr: 8 LE] [offset: 2 LE] [size: 1] [num: 1]
            cmd::PEEK if payload.len() >= 12 => {
                let addr   = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let offset = u16::from_le_bytes(payload[8..10].try_into().unwrap());
                let size   = payload[10];
                let num    = payload[11];
                RxCmd::Peek { addr, offset, size, num }
            }

            // POKE: [addr: 8 LE] [offset: 2 LE] [size: 1] [num: 1] [data: size*num]
            cmd::POKE if payload.len() >= 12 => {
                let addr   = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let offset = u16::from_le_bytes(payload[8..10].try_into().unwrap());
                let size   = payload[10];
                let num    = payload[11];
                let data   = payload[12..].to_vec();
                RxCmd::Poke { addr, offset, size, num, data }
            }

            // FILL: [addr: 8 LE] [offset: 2 LE] [size: 1] [num: 1] [data: size]
            cmd::FILL if payload.len() >= 12 => {
                let addr   = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                let offset = u16::from_le_bytes(payload[8..10].try_into().unwrap());
                let size   = payload[10];
                let num    = payload[11];
                let data   = payload[12..].to_vec();
                RxCmd::Fill { addr, offset, size, num, data }
            }

            // EVENT: [prio: 1] [signal: 2 LE] [payload: ...]
            cmd::EVENT if payload.len() >= 3 => {
                let prio   = payload[0];
                let signal = u16::from_le_bytes(payload[1..3].try_into().unwrap());
                RxCmd::Event { prio, signal, payload: payload[3..].to_vec() }
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
        raw.push(!sum);

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
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Info]);
    }

    #[test]
    fn decode_reset_command() {
        let frame = encode_frame(2, cmd::RESET, &[]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Reset]);
    }

    #[test]
    fn decode_tick_command() {
        let frame = encode_frame(3, cmd::TICK, &[1]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Tick { rate: 1 }]);
    }

    #[test]
    fn decode_ao_filter() {
        let frame = encode_frame(4, cmd::AO_FILTER, &[7]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::AoFilter { prio: 7 }]);
    }

    #[test]
    fn decode_glb_filter() {
        let bits = [0xFFu8; 16];
        let frame = encode_frame(5, cmd::GLB_FILTER, &bits);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::GlbFilter { bits }]);
    }

    #[test]
    fn decode_test_setup() {
        let frame = encode_frame(1, cmd::TEST_SETUP, &[]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::TestSetup]);
    }

    #[test]
    fn decode_test_teardown() {
        let frame = encode_frame(1, cmd::TEST_TEARDOWN, &[]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::TestTeardown]);
    }

    #[test]
    fn decode_test_continue() {
        let frame = encode_frame(1, cmd::TEST_CONTINUE, &[]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::TestContinue]);
    }

    #[test]
    fn decode_test_probe() {
        let fn_ptr: u64 = 0x0102030405060708;
        let data:   u32 = 0x0A0B0C0D;
        let mut payload = [0u8; 12];
        payload[0..8].copy_from_slice(&fn_ptr.to_le_bytes());
        payload[8..12].copy_from_slice(&data.to_le_bytes());
        let frame = encode_frame(1, cmd::TEST_PROBE, &payload);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::TestProbe { fn_ptr, data }]);
    }

    #[test]
    fn decode_command() {
        let mut payload = [0u8; 13];
        payload[0] = 5;
        payload[1..5].copy_from_slice(&42u32.to_le_bytes());
        payload[5..9].copy_from_slice(&100u32.to_le_bytes());
        payload[9..13].copy_from_slice(&0u32.to_le_bytes());
        let frame = encode_frame(1, cmd::COMMAND, &payload);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Command { id: 5, p1: 42, p2: 100, p3: 0 }]);
    }

    #[test]
    fn decode_curr_obj() {
        let mut payload = [0u8; 9];
        payload[0] = 1;
        payload[1..9].copy_from_slice(&0xDEADBEEFu64.to_le_bytes());
        let frame = encode_frame(1, cmd::CURR_OBJ, &payload);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::CurrObj { kind: 1, obj_ptr: 0xDEADBEEF }]);
    }

    #[test]
    fn decode_query_curr() {
        let frame = encode_frame(1, cmd::QUERY_CURR, &[2]);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::QueryCurr { kind: 2 }]);
    }

    #[test]
    fn decode_event() {
        let payload = vec![3u8, 0x05, 0x00, 0xAB, 0xCD];
        let frame = encode_frame(1, cmd::EVENT, &payload);
        let cmds = RxParser::new().push_slice(&frame);
        assert_eq!(cmds, vec![RxCmd::Event { prio: 3, signal: 5, payload: vec![0xAB, 0xCD] }]);
        let _ = payload;
    }

    #[test]
    fn bad_checksum_discarded() {
        let mut frame = encode_frame(1, cmd::INFO, &[]);
        let last = frame.len() - 2;
        frame[last] ^= 0x01;
        let cmds = RxParser::new().push_slice(&frame);
        assert!(cmds.is_empty());
    }

    #[test]
    fn two_frames_back_to_back() {
        let mut data = encode_frame(1, cmd::INFO, &[]);
        data.extend(encode_frame(2, cmd::RESET, &[]));
        let cmds = RxParser::new().push_slice(&data);
        assert_eq!(cmds, vec![RxCmd::Info, RxCmd::Reset]);
    }
}
