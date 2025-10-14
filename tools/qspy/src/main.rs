//! QSpy - Software Tracing Host Utility
//!
//! Receives, interprets, and displays QS trace records from embedded targets

mod protocol;
mod parser;
mod formatter;

use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use colored::Colorize;
use std::net::UdpSocket;
use std::time::Duration;

use protocol::{QSPY_UDP_PORT, QSPY_VERSION, QSPY_TIMEOUT_SEC};
use parser::QSParser;
use formatter::RecordFormatter;

#[derive(ClapParser, Debug)]
#[command(name = "qspy")]
#[command(about = "QSpy Software Tracing Host Utility", long_about = None)]
#[command(version)]
struct Args {
    /// UDP port to listen on
    #[arg(short, long, default_value_t = QSPY_UDP_PORT)]
    port: u16,

    /// Local host address to bind to
    #[arg(short, long, default_value = "0.0.0.0")]
    local: String,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Show timestamps
    #[arg(short, long)]
    timestamps: bool,

    /// Output format (text, json)
    #[arg(short = 'f', long, default_value = "text")]
    format: String,

    /// Filter by record group (sm, ao, eq, mp, te, sched, user)
    #[arg(long)]
    filter: Option<Vec<String>>,
}

fn print_banner() {
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║              QSpy Software Tracing Utility             ║");
    println!("║              Version {}.{}.{} (Rust)                         ║", 
        QSPY_VERSION / 100, 
        (QSPY_VERSION / 10) % 10, 
        QSPY_VERSION % 10);
    println!("║       Copyright (c) 2005-2025 Quantum Leaps           ║");
    println!("║              www.state-machine.com                     ║");
    println!("╚════════════════════════════════════════════════════════╝\n");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    print_banner();

    // Create UDP socket for receiving traces
    let bind_addr = format!("{}:{}", args.local, args.port);
    println!("📡 Binding to UDP socket: {}", bind_addr.bright_cyan());
    
    let socket = UdpSocket::bind(&bind_addr)
        .context(format!("Failed to bind UDP socket to {}", bind_addr))?;
    
    socket.set_read_timeout(Some(Duration::from_secs(QSPY_TIMEOUT_SEC)))
        .context("Failed to set socket timeout")?;

    println!("✓ Socket ready, listening for QS traces...");
    println!("  Press {} to stop\n", "Ctrl-C".bright_yellow());

    let mut parser = QSParser::new();
    let mut formatter = RecordFormatter::new(args.timestamps, args.format == "json");

    // Apply filters if provided
    if let Some(filters) = args.filter {
        formatter.set_filters(&filters);
        println!("🔍 Filters applied: {}\n", filters.join(", ").bright_green());
    }

    let mut buf = vec![0u8; 65536]; // 64KB buffer
    let mut packet_count: u64 = 0;
    let mut record_count: u64 = 0;

    // Setup Ctrl-C handler
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        tx.send(()).await.ok();
    });

    // Main receive loop
    loop {
        // Check for Ctrl-C
        if rx.try_recv().is_ok() {
            println!("\n\n{} signal received, shutting down...", "Ctrl-C".bright_yellow());
            break;
        }

        // Receive data with timeout
        match socket.recv_from(&mut buf) {
            Ok((size, src_addr)) => {
                packet_count += 1;
                
                if args.verbose {
                    println!("📦 Packet #{} from {} ({} bytes)", 
                        packet_count.to_string().bright_blue(),
                        src_addr.to_string().bright_magenta(), 
                        size);
                }

                // Parse the packet
                let packet_data = &buf[..size];
                if let Some(records) = parser.parse_packet(packet_data) {
                    for record in records {
                        record_count += 1;
                        formatter.format_record(&record);
                    }
                } else if args.verbose {
                    println!("⚠ Failed to parse packet");
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Timeout - check for Ctrl-C and continue
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                eprintln!("❌ Socket error: {}", e);
                break;
            }
        }
    }

    // Print statistics
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║                     Statistics                          ║");
    println!("╠════════════════════════════════════════════════════════╣");
    println!("║  Packets received:  {:>10}                        ║", packet_count.to_string().bright_cyan());
    println!("║  Records processed: {:>10}                        ║", record_count.to_string().bright_green());
    println!("╚════════════════════════════════════════════════════════╝\n");

    Ok(())
}
