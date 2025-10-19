use std::error::Error;
use std::io::{self};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::thread;

use clap::Parser;
use qspy::{FrameInterpreter, HdlcDecoder};

#[derive(Parser, Debug)]
#[command(author, version, about = "Rust reimplementation of Quantum Spy")]
struct Opts {
    #[arg(long = "udp", default_value = "0.0.0.0:7701", value_name = "ADDR")]
    udp_addr: String,

    #[arg(long = "cmd", default_value = "127.0.0.1:6601", value_name = "ADDR")]
    cmd_addr: String,

    #[arg(long = "no-cmd")]
    no_cmd: bool,
}

impl Opts {
    fn command_address(&self) -> Option<&str> {
        if self.no_cmd {
            None
        } else {
            Some(&self.cmd_addr)
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts = Opts::parse();

    if let Some(addr) = opts.command_address() {
        let addr = addr.to_string();
        thread::spawn(move || {
            if let Err(err) = run_command_listener(&addr) {
                eprintln!("command listener error: {err}");
            }
        });
    }

    let socket = UdpSocket::bind(&opts.udp_addr)?;
    println!("qspy listening on udp://{}", opts.udp_addr);

    let mut decoder = HdlcDecoder::new();
    let mut interpreter = FrameInterpreter::new();
    let mut buf = [0u8; 4096];
    let mut last_peer: Option<String> = None;

    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, peer)) => {
                let peer_str = peer.to_string();
                if last_peer.as_deref() != Some(peer_str.as_str()) {
                    println!("telemetry from {peer}");
                }
                last_peer = Some(peer_str);

                match decoder.push_bytes(&buf[..len]) {
                    Ok(frames) => {
                        for frame in frames {
                            for line in interpreter.interpret(&frame) {
                                println!("{line}");
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("decoder error: {err}; resetting state");
                        decoder.reset();
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => {
                eprintln!("socket error: {err}");
                break;
            }
        }
    }

    Ok(())
}

fn run_command_listener(addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("command listener on tcp://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let peer = stream.peer_addr().ok();
                println!("command channel connected: {peer:?}");
                thread::spawn(move || {
                    if let Err(err) = handle_command_stream(stream) {
                        eprintln!("command channel error: {err}");
                    }
                });
            }
            Err(err) => eprintln!("command accept error: {err}"),
        }
    }

    Ok(())
}

fn handle_command_stream(mut stream: TcpStream) -> io::Result<()> {
    stream.set_nodelay(true).ok();
    let peer = stream.peer_addr().ok();
    let mut sink = io::sink();
    let result = io::copy(&mut stream, &mut sink);
    println!("command channel closed: {peer:?}");
    result.map(|_| ())
}
