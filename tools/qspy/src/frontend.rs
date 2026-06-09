use std::io;
use std::net::{SocketAddr, UdpSocket};

use crate::commands::{
    QS_RX_AO_FILTER, QS_RX_CURR_OBJ, QS_RX_EVENT, QS_RX_TEST_PROBE,
};

// QSPY back-end command record IDs (128–255).
pub const QSPY_ATTACH:            u8 = 128;
pub const QSPY_DETACH:            u8 = 129;
pub const QSPY_SAVE_DICT:         u8 = 130;
pub const QSPY_TEXT_OUT:          u8 = 131;
pub const QSPY_BIN_OUT:           u8 = 132;
pub const QSPY_SEND_EVENT:        u8 = 135;
pub const QSPY_SEND_AO_FILTER:    u8 = 136;
pub const QSPY_SEND_CURR_OBJ:     u8 = 137;
pub const QSPY_SEND_COMMAND:      u8 = 138;
pub const QSPY_SEND_TEST_PROBE:   u8 = 139;
pub const QSPY_CLEAR_SCREEN:      u8 = 140;
pub const QSPY_SHOW_NOTE:         u8 = 141;

pub const CHANNEL_BINARY: u8 = 0x01;
pub const CHANNEL_TEXT:   u8 = 0x02;

/// Commands extracted from incoming front-end UDP packets that need
/// to be forwarded to the target or acted on locally.
pub enum FrontendCmd {
    Command { id: u8, p1: u32, p2: u32, p3: u32 },
    SaveDict,
    ClearScreen,
    /// Request a fresh TARGET_INFO from the target (triggered on ATTACH, GAP-9).
    Info,
    /// Toggle the text output file open/closed.
    ToggleTextOut,
    /// Toggle the binary save file open/closed.
    ToggleBinOut,
    /// Print a note string as a console/text-file line.
    ShowNote(String),
    /// Raw QS-RX passthrough: forward the frame verbatim to the target's command channel.
    RawQsRx { id: u8, payload: Vec<u8> },
}

struct Client {
    addr:     SocketAddr,
    channels: u8,
    seq:      u8,
}

/// UDP server that implements the QSPY back-end protocol for front-ends
/// (QView, QUTest).  Runs entirely on the caller's thread using a
/// non-blocking socket — call `poll()` once per input-loop iteration.
pub struct FrontendServer {
    socket:  UdpSocket,
    clients: Vec<Client>,
}

impl FrontendServer {
    pub fn bind(addr: &str) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;
        println!("front-end server on udp://{addr}");
        Ok(Self { socket, clients: Vec::new() })
    }

    /// Drain all pending incoming datagrams and return any commands that
    /// should be forwarded to the target or acted on locally.
    pub fn poll(&mut self) -> Vec<FrontendCmd> {
        let mut cmds = Vec::new();
        let mut buf = [0u8; 2048];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, peer)) => {
                    cmds.extend(self.handle_packet(&buf[..len], peer));
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    eprintln!("front-end recv error: {e}");
                    break;
                }
            }
        }
        cmds
    }

    /// Forward a decoded QS frame to all binary-channel clients.
    pub fn forward_frame(&mut self, record_type: u8, payload: &[u8]) {
        let mut i = 0;
        while i < self.clients.len() {
            if self.clients[i].channels & CHANNEL_BINARY == 0 {
                i += 1;
                continue;
            }
            let seq = self.clients[i].seq;
            self.clients[i].seq = seq.wrapping_add(1);
            let addr = self.clients[i].addr;

            let mut pkt = Vec::with_capacity(2 + payload.len());
            pkt.push(seq);
            pkt.push(record_type);
            pkt.extend_from_slice(payload);

            if self.socket.send_to(&pkt, addr).is_err() {
                self.clients.remove(i);
                continue;
            }
            i += 1;
        }
    }

    /// Forward a decoded text line to all text-channel clients.
    pub fn forward_text(&mut self, line: &str) {
        let mut i = 0;
        while i < self.clients.len() {
            if self.clients[i].channels & CHANNEL_TEXT == 0 {
                i += 1;
                continue;
            }
            let seq = self.clients[i].seq;
            self.clients[i].seq = seq.wrapping_add(1);
            let addr = self.clients[i].addr;

            let mut pkt = Vec::with_capacity(2 + line.len() + 1);
            pkt.push(seq);
            pkt.push(0x00); // text record sentinel
            pkt.extend_from_slice(line.as_bytes());
            pkt.push(b'\n');

            if self.socket.send_to(&pkt, addr).is_err() {
                self.clients.remove(i);
                continue;
            }
            i += 1;
        }
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn handle_packet(&mut self, data: &[u8], peer: SocketAddr) -> Vec<FrontendCmd> {
        if data.len() < 2 {
            return vec![];
        }
        let fe_seq    = data[0];
        let record_id = data[1];
        let payload   = &data[2..];

        match record_id {
            QSPY_ATTACH => {
                let channels = payload.first().copied().unwrap_or(CHANNEL_BINARY);
                if let Some(c) = self.clients.iter_mut().find(|c| c.addr == peer) {
                    c.channels = channels;
                } else {
                    self.clients.push(Client { addr: peer, channels, seq: 0 });
                }
                // ACK: echo the ATTACH packet back to the client.
                let ack = [fe_seq, QSPY_ATTACH];
                let _ = self.socket.send_to(&ack, peer);
                println!("front-end attached: {peer} channels={channels:#04x}");
                // GAP-9: request a fresh TARGET_INFO so the front-end gets current sizes.
                vec![FrontendCmd::Info]
            }
            QSPY_DETACH => {
                self.clients.retain(|c| c.addr != peer);
                println!("front-end detached: {peer}");
                vec![]
            }
            QSPY_SAVE_DICT    => vec![FrontendCmd::SaveDict],
            QSPY_CLEAR_SCREEN => vec![FrontendCmd::ClearScreen],
            QSPY_TEXT_OUT     => vec![FrontendCmd::ToggleTextOut],
            QSPY_BIN_OUT      => vec![FrontendCmd::ToggleBinOut],
            QSPY_SHOW_NOTE    => {
                let s = String::from_utf8_lossy(payload)
                    .trim_end_matches('\0')
                    .to_string();
                vec![FrontendCmd::ShowNote(s)]
            }

            // QSPY backend commands that map to QS-RX target commands.
            // The front-end already encodes fields using the correct target sizes,
            // so we forward the payload bytes verbatim.
            QSPY_SEND_EVENT      => vec![FrontendCmd::RawQsRx {
                id: QS_RX_EVENT,     payload: payload.to_vec()
            }],
            QSPY_SEND_AO_FILTER  => vec![FrontendCmd::RawQsRx {
                id: QS_RX_AO_FILTER, payload: payload.to_vec()
            }],
            QSPY_SEND_CURR_OBJ   => vec![FrontendCmd::RawQsRx {
                id: QS_RX_CURR_OBJ,  payload: payload.to_vec()
            }],
            QSPY_SEND_TEST_PROBE => vec![FrontendCmd::RawQsRx {
                id: QS_RX_TEST_PROBE, payload: payload.to_vec()
            }],

            // QSPY_SEND_COMMAND: payload = [id:u8, p1:u32le, p2:u32le, p3:u32le]
            QSPY_SEND_COMMAND if payload.len() >= 13 => {
                let id = payload[0];
                let p1 = u32::from_le_bytes(payload[1..5].try_into().unwrap());
                let p2 = u32::from_le_bytes(payload[5..9].try_into().unwrap());
                let p3 = u32::from_le_bytes(payload[9..13].try_into().unwrap());
                vec![FrontendCmd::Command { id, p1, p2, p3 }]
            }

            // QS-RX passthrough: forward ALL records 0–127 verbatim to the target.
            rec if rec < 128 => {
                vec![FrontendCmd::RawQsRx { id: rec, payload: payload.to_vec() }]
            }
            _ => vec![],
        }
    }
}
