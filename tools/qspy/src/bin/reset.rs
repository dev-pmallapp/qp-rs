//! QSpy-Reset Utility
//!
//! Sends a reset command to the target through QSpy via UDP

use anyhow::Result;
use clap::Parser;
use std::net::UdpSocket;
use std::time::Duration;

const QSPY_VERSION: u16 = 810;
const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 7701;
const TIMEOUT_SEC: u64 = 1;

// Target command codes
const TO_TRG_RESET: u8 = 2;

#[derive(Parser, Debug)]
#[command(name = "qspy-reset")]
#[command(about = "Reset the target through QSpy", long_about = None)]
#[command(version)]
struct Args {
    /// QSpy host address (format: host:port or just host)
    #[arg(short, long, default_value = DEFAULT_HOST)]
    qspy: String,
}

fn parse_qspy_address(input: &str) -> (String, u16) {
    if let Some((host, port_str)) = input.split_once(':') {
        let port = port_str.parse::<u16>().unwrap_or(DEFAULT_PORT);
        (host.to_string(), port)
    } else {
        (input.to_string(), DEFAULT_PORT)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    
    println!("\nQSPY-reset {}.{}.{}", 
        QSPY_VERSION / 100, 
        (QSPY_VERSION / 10) % 10, 
        QSPY_VERSION % 10);
    println!("Copyright (c) 2005-2025 Quantum Leaps (Rust port)");
    println!("www.state-machine.com\n");

    let (host, port) = parse_qspy_address(&args.qspy);
    let qspy_addr = format!("{}:{}", host, port);

    println!("Connecting to QSpy at {}...", qspy_addr);

    // Create UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_write_timeout(Some(Duration::from_secs(TIMEOUT_SEC)))?;

    // Send RESET command to target through QSpy
    let mut tx_packet = vec![0u8]; // sequence number (0 for first packet)
    tx_packet.push(TO_TRG_RESET);

    socket.send_to(&tx_packet, &qspy_addr)?;
    
    println!("Reset command sent to target through QSpy at {}", qspy_addr);
    println!("✓ Success\n");

    Ok(())
}
