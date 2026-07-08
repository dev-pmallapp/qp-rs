use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
pub const QS_RX_INFO:           u8 = 0;
pub const QS_RX_COMMAND:        u8 = 1;
pub const QS_RX_RESET:          u8 = 2;
pub const QS_RX_TICK:           u8 = 3;
#[allow(dead_code)] pub const QS_RX_PEEK:           u8 = 4;
#[allow(dead_code)] pub const QS_RX_POKE:           u8 = 5;
#[allow(dead_code)] pub const QS_RX_FILL:           u8 = 6;
#[allow(dead_code)] pub const QS_RX_TEST_SETUP:     u8 = 7;
#[allow(dead_code)] pub const QS_RX_TEST_TEARDOWN:  u8 = 8;
#[allow(dead_code)] pub const QS_RX_TEST_PROBE:     u8 = 9;
#[allow(dead_code)] pub const QS_RX_GLB_FILTER:     u8 = 10;
#[allow(dead_code)] pub const QS_RX_LOC_FILTER:     u8 = 11;
#[allow(dead_code)] pub const QS_RX_AO_FILTER:      u8 = 12;
#[allow(dead_code)] pub const QS_RX_CURR_OBJ:       u8 = 13;
#[allow(dead_code)] pub const QS_RX_CONTINUE:       u8 = 14;
#[allow(dead_code)] pub const QS_RX_QUERY_CURR:     u8 = 15;
#[allow(dead_code)] pub const QS_RX_EVENT:          u8 = 16;

const FLAG: u8 = 0x7E;
const ESC:  u8 = 0x7D;
const XOR:  u8 = 0x20;

/// Shared handle to the target's command stream (set when target connects).
pub type SharedSender = Arc<Mutex<Option<CommandSender>>>;

pub struct CommandSender {
    writer: Box<dyn Write + Send>,
    seq:    u8,
}

impl CommandSender {
    /// Wraps any duplex byte sink (a cloned `TcpStream`, a serial `File`,
    /// …) as a QS-RX command sender. Callers that need transport-specific
    /// setup (e.g. `TcpStream::set_nodelay`) do it before boxing.
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self { writer, seq: 0 }
    }

    pub fn send_info(&mut self) -> io::Result<()> {
        self.send(QS_RX_INFO, &[])
    }

    pub fn send_reset(&mut self) -> io::Result<()> {
        self.send(QS_RX_RESET, &[])
    }

    pub fn send_tick(&mut self, rate: u8) -> io::Result<()> {
        self.send(QS_RX_TICK, &[rate])
    }

    /// Forward a pre-built QS-RX payload verbatim (used by the raw front-end passthrough).
    pub fn send_raw(&mut self, record_id: u8, payload: &[u8]) -> io::Result<()> {
        self.send(record_id, payload)
    }

    #[allow(dead_code)]
    pub fn send_test_setup(&mut self) -> io::Result<()> {
        self.send(QS_RX_TEST_SETUP, &[])
    }

    #[allow(dead_code)]
    pub fn send_test_teardown(&mut self) -> io::Result<()> {
        self.send(QS_RX_TEST_TEARDOWN, &[])
    }

    #[allow(dead_code)]
    pub fn send_continue(&mut self) -> io::Result<()> {
        self.send(QS_RX_CONTINUE, &[])
    }

    #[allow(dead_code)]
    pub fn send_ao_filter(&mut self, prio: u8) -> io::Result<()> {
        self.send(QS_RX_AO_FILTER, &[prio])
    }

    #[allow(dead_code)]
    pub fn send_query_curr(&mut self, kind: u8) -> io::Result<()> {
        self.send(QS_RX_QUERY_CURR, &[kind])
    }

    pub fn send_command(&mut self, id: u8, p1: u32, p2: u32, p3: u32) -> io::Result<()> {
        let mut payload = [0u8; 13];
        payload[0] = id;
        payload[1..5].copy_from_slice(&p1.to_le_bytes());
        payload[5..9].copy_from_slice(&p2.to_le_bytes());
        payload[9..13].copy_from_slice(&p3.to_le_bytes());
        self.send(QS_RX_COMMAND, &payload)
    }

    #[allow(dead_code)]
    pub fn send_glb_filter(&mut self, mask: &[u8; 16]) -> io::Result<()> {
        let mut payload = [0u8; 17];
        payload[0] = 16;
        payload[1..].copy_from_slice(mask);
        self.send(QS_RX_GLB_FILTER, &payload)
    }

    #[allow(dead_code)]
    pub fn send_loc_filter(&mut self, mask: &[u8; 16]) -> io::Result<()> {
        let mut payload = [0u8; 17];
        payload[0] = 16;
        payload[1..].copy_from_slice(mask);
        self.send(QS_RX_LOC_FILTER, &payload)
    }

    fn send(&mut self, record_id: u8, payload: &[u8]) -> io::Result<()> {
        let frame = build_frame(self.seq, record_id, payload);
        self.seq = self.seq.wrapping_add(1);
        self.writer.write_all(&frame)
    }
}

/// HDLC-encode a QS-RX frame: `FLAG [seq] [record_id] [payload] [chk] FLAG`
fn build_frame(seq: u8, record_id: u8, payload: &[u8]) -> Vec<u8> {
    // Assemble raw bytes: seq + record_id + payload
    let mut raw: Vec<u8> = Vec::with_capacity(2 + payload.len() + 1);
    raw.push(seq);
    raw.push(record_id);
    raw.extend_from_slice(payload);

    // Checksum = bitwise-NOT of the running sum over all raw bytes.
    let sum: u8 = raw.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    raw.push(!sum);

    // HDLC byte-stuffing inside FLAG delimiters.
    let mut out = Vec::with_capacity(raw.len() + 4);
    out.push(FLAG);
    for byte in raw {
        if byte == FLAG || byte == ESC {
            out.push(ESC);
            out.push(byte ^ XOR);
        } else {
            out.push(byte);
        }
    }
    out.push(FLAG);
    out
}

/// Try to send a command via the shared sender; silently clears the sender on error.
pub fn try_send<F>(sender: &SharedSender, f: F)
where
    F: FnOnce(&mut CommandSender) -> io::Result<()>,
{
    let mut guard = sender.lock().unwrap();
    if let Some(s) = guard.as_mut() {
        if let Err(e) = f(s) {
            eprintln!("cmd send error: {e}; command channel dropped");
            *guard = None;
        }
    } else {
        eprintln!("no target connected on command channel");
    }
}
