use crate::*;
use std::env;
use std::io::Read;
use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use qf::active::ActiveObjectId;
use qf::event::{DynEvent, Signal};
use qf_port_posix::PosixPort;
use qs::rx::{cmd as rx_cmd, RxCmd, RxParser};
use qs::{clear_test_probes, set_test_probe, GlbFilter, TargetInfo};

pub(crate) fn init_port() -> Arc<PosixPort> {
    let cmd_addr = env::var("QSPY_CMD_ADDR").unwrap_or_else(|_| "127.0.0.1:6601".to_string());
    let port = if let Ok(raw_addr) = env::var("QSPY_ADDR") {
        let addr = raw_addr.trim().to_string();
        match PosixPort::connect(&addr) {
            Ok(port) => {
                println!("QS tracing connected to tcp://{addr}");
                port
            }
            Err(err) => {
                eprintln!(
                    "failed to connect to qspy at {addr}: {err}; falling back to UDP default"
                );
                connect_udp_default()
            }
        }
    } else {
        connect_udp_default()
    };

    let port = Arc::new(port);
    PORT.set(Arc::clone(&port)).unwrap_or_else(|_| panic!("port already set"));
    start_command_channel(&cmd_addr, Arc::clone(&port));
    port
}

fn connect_udp_default() -> PosixPort {
    let udp_addr = env::var("QSPY_UDP_ADDR").unwrap_or_else(|_| "127.0.0.1:7701".to_string());
    match PosixPort::connect_udp(&udp_addr) {
        Ok(port) => {
            println!("QS tracing connected to udp://{udp_addr}");
            port
        }
        Err(err) => {
            eprintln!("failed to connect to qspy at {udp_addr}: {err}; falling back to stdout");
            PosixPort::new()
        }
    }
}

pub(crate) fn start_command_channel(addr: &str, port: Arc<PosixPort>) {
    let addr = addr.to_string();
    thread::spawn(move || loop {
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                if let Err(err) = stream.set_nodelay(true) {
                    eprintln!("failed to configure QS command channel: {err}");
                }
                handle_command_stream(stream, Arc::clone(&port));
            }
            Err(err) => {
                eprintln!("failed to connect to QS command listener at {addr}: {err}");
            }
        }

        thread::sleep(Duration::from_secs(1));
    });
}

fn handle_command_stream(mut stream: TcpStream, port: Arc<PosixPort>) {
    if let Ok(peer) = stream.peer_addr() {
        println!("QS command channel connected to {peer}");
    }
    let mut buffer = [0u8; 128];
    let mut ctx = QsRxContext::new(port);
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                for &byte in &buffer[..count] {
                    ctx.ingest(byte);
                }
            }
            Err(err) => {
                eprintln!("QS command stream error: {err}");
                break;
            }
        }
    }
    if let Ok(peer) = stream.peer_addr() {
        println!("QS command channel from {peer} closed");
    }
}

struct QsRxContext {
    port:   Arc<PosixPort>,
    parser: RxParser,
}

impl QsRxContext {
    fn new(port: Arc<PosixPort>) -> Self {
        Self { port, parser: RxParser::new() }
    }

    fn ingest(&mut self, byte: u8) {
        if let Some(cmd) = self.parser.push(byte) {
            self.handle_cmd(cmd);
        }
    }

    fn handle_cmd(&self, cmd: RxCmd) {
        match cmd {
            RxCmd::Info => {
                if let Err(err) = self.port.emit_target_info(&TargetInfo::default()) {
                    eprintln!("QS-RX INFO error: {err}");
                }
            }
            RxCmd::Reset => {
                eprintln!("QS-RX RESET (not implemented in this demo)");
            }
            RxCmd::Command { id, p1, p2, p3 } => {
                self.ack(rx_cmd::COMMAND);
                println!("QS command id={id} params=[{p1}, {p2}, {p3}]");
                self.done(rx_cmd::COMMAND);
            }
            RxCmd::TestSetup => {
                clear_test_probes();
                self.ack_done(rx_cmd::TEST_SETUP);
            }
            RxCmd::TestTeardown => {
                clear_test_probes();
                self.ack_done(rx_cmd::TEST_TEARDOWN);
            }
            RxCmd::TestContinue => {
                self.ack_done(rx_cmd::TEST_CONTINUE);
            }
            RxCmd::TestProbe { fn_ptr, data } => {
                set_test_probe(fn_ptr, data);
                self.ack_done(rx_cmd::TEST_PROBE);
            }
            RxCmd::Event { prio, signal, .. } => {
                if let Some(kernel) = KERNEL.get() {
                    let _ = kernel.post(
                        ActiveObjectId::new(prio),
                        DynEvent::empty_dyn(Signal(signal)),
                    );
                }
                self.ack_done(rx_cmd::EVENT);
            }
            RxCmd::GlbFilter { bits } => {
                self.port.set_filter(GlbFilter::from_bytes(bits));
                self.ack_done(rx_cmd::GLB_FILTER);
            }
            RxCmd::Tick { .. }       => self.ack_done(rx_cmd::TICK),
            RxCmd::AoFilter { .. }   => self.ack_done(rx_cmd::AO_FILTER),
            RxCmd::LocFilter { .. }  => self.ack_done(rx_cmd::LOC_FILTER),
            RxCmd::CurrObj { .. }    => self.ack_done(rx_cmd::CURR_OBJ),
            RxCmd::QueryCurr { .. }  => self.ack_done(rx_cmd::QUERY_CURR),
            RxCmd::Peek { .. }       => self.ack_done(rx_cmd::PEEK),
            RxCmd::Poke { .. }       => self.ack_done(rx_cmd::POKE),
            RxCmd::Fill { .. }       => self.ack_done(rx_cmd::FILL),
            RxCmd::Unknown { cmd, .. } => {
                eprintln!("unknown QS-RX record {cmd:#04x}");
                let _ = self.port.emit_record(QS_RX_STATUS, &[0x80 | 0x43u8], false);
            }
        }
    }

    fn ack(&self, rec_id: u8) {
        let _ = self.port.emit_record(QS_RX_STATUS, &[rec_id], false);
    }

    fn done(&self, rec_id: u8) {
        let _ = self.port.emit_record(QS_TARGET_DONE, &[rec_id], true);
    }

    fn ack_done(&self, rec_id: u8) {
        self.ack(rec_id);
        self.done(rec_id);
    }
}
