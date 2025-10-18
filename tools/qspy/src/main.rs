use std::error::Error;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use qspy::{HdlcDecoder, QsFrame};

fn main() -> Result<(), Box<dyn Error>> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:7701".to_string());

    let listener = TcpListener::bind(&addr)?;
    println!("qspy listening on {addr}");

    for connection in listener.incoming() {
        match connection {
            Ok(stream) => {
                let peer = stream.peer_addr().ok();
                println!("client connected: {:?}", peer);
                if let Err(err) = handle_stream(stream) {
                    eprintln!("connection error: {err}");
                }
            }
            Err(err) => eprintln!("accept error: {err}"),
        }
    }

    Ok(())
}

fn handle_stream(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    let mut decoder = HdlcDecoder::new();
    let mut buf = [0u8; 1024];

    loop {
        let read = stream.read(&mut buf)?;
        if read == 0 {
            break;
        }

        match decoder.push_bytes(&buf[..read]) {
            Ok(frames) => {
                for frame in frames {
                    print_frame(&frame);
                }
            }
            Err(err) => {
                eprintln!("decoder error: {err}; resetting state");
                decoder.reset();
            }
        }
    }

    let _ = stream.write_all(b"BYE\n");
    Ok(())
}

fn print_frame(frame: &QsFrame) {
    print!(
        "seq={:03} type=0x{:02X} payload=",
        frame.seq, frame.record_type
    );
    for byte in &frame.payload {
        print!("{:02X}", byte);
    }
    println!();
}
