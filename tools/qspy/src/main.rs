//! QSpy - Software Tracing Host Utility
//!
//! Receives, interprets, and displays QS trace records from embedded targets

mod protocol;
mod parser;
mod formatter;
mod decoder;
mod commands;
mod keyboard;

use anyhow::{Context, Result};
use clap::Parser as ClapParser;
use colored::Colorize;
use std::net::{UdpSocket, TcpListener, TcpStream};
use std::time::Duration;
use std::io::{Read, Write};

use protocol::{QSPY_UDP_PORT, QSPY_VERSION, QSPY_TIMEOUT_SEC};
use parser::QSParser;

/// Macro for printing with proper line endings in raw terminal mode
/// Uses \r\n instead of \n to ensure cursor returns to column 0
macro_rules! raw_println {
    () => {
        print!("\r\n");
        let _ = std::io::stdout().flush();
    };
    ($($arg:tt)*) => {{
        print!($($arg)*);
        print!("\r\n");
        let _ = std::io::stdout().flush();
    }};
}
use formatter::RecordFormatter;

#[derive(ClapParser, Debug)]
#[command(name = "qspy")]
#[command(about = "QSpy Software Tracing Host Utility", long_about = None)]
#[command(version = "8.1.1")]
struct Args {
    // Input/Output Options
    /// Quiet mode - suppress QS data output (key-q to toggle)
    #[arg(short = 'q', long)]
    quiet: Option<Option<u8>>,

    /// UDP socket port (default 7701, 0 to disable)
    #[arg(short = 'u', long)]
    udp: Option<Option<u16>>,

    /// License file path (*.qlc)
    #[arg(short = 'l', long)]
    license: Option<String>,

    /// QS version compatibility (>= 6.6)
    #[arg(short = 'v', long, default_value = "7.0")]
    qs_version: String,

    /// Rendering mode: c0=no-color, c1=color1, c2=color2
    #[arg(short = 'r', long, default_value = "c1")]
    rendering: String,

    /// Suppress keyboard input (default: keyboard enabled)
    #[arg(short = 'k', long)]
    no_keyboard: bool,

    /// Save screen output to file (key-o to toggle)
    #[arg(short = 'o', long)]
    screen_output: bool,

    /// Save binary QS data to file (key-s to toggle)
    #[arg(short = 's', long)]
    binary_output: bool,

    /// Produce Matlab output to file
    #[arg(short = 'm', long)]
    matlab_output: bool,

    /// Produce sequence diagram to file with object list
    #[arg(short = 'g', long)]
    sequence_diagram: Option<String>,

    // Input Sources
    /// TCP/IP port to listen on (default 6601)
    #[arg(short = 't', long)]
    tcp: Option<Option<u16>>,

    /// Serial port input (e.g., /dev/ttyS0, COM3)
    #[arg(short = 'c', long)]
    com_port: Option<String>,

    /// Baud rate for serial port (default 115200)
    #[arg(short = 'b', long, default_value_t = 115200)]
    baud_rate: u32,

    /// File input for postprocessing
    #[arg(short = 'f', long)]
    file_input: Option<String>,

    /// Dictionary file(s) to load
    #[arg(short = 'd', long)]
    dictionary: Option<Option<String>>,

    // Size Configuration Overrides
    /// QS timestamp size in bytes (default 4)
    #[arg(short = 'T', long, default_value_t = 4)]
    timestamp_size: u8,

    /// Object pointer size in bytes (default 4)
    #[arg(short = 'O', long, default_value_t = 4)]
    obj_ptr_size: u8,

    /// Function pointer size in bytes (default 4)
    #[arg(short = 'F', long, default_value_t = 4)]
    fun_ptr_size: u8,

    /// Event signal size in bytes (default 2)
    #[arg(short = 'S', long, default_value_t = 2)]
    signal_size: u8,

    /// Event size field size in bytes (default 2)
    #[arg(short = 'E', long, default_value_t = 2)]
    event_size: u8,

    /// Queue counter size in bytes (default 1)
    #[arg(short = 'Q', long, default_value_t = 1)]
    queue_ctr_size: u8,

    /// Pool counter size in bytes (default 2)
    #[arg(short = 'P', long, default_value_t = 2)]
    pool_ctr_size: u8,

    /// Pool block-size field size in bytes (default 2)
    #[arg(short = 'B', long, default_value_t = 2)]
    pool_blk_size: u8,

    /// QTimeEvt counter size in bytes (default 4)
    #[arg(short = 'C', long, default_value_t = 4)]
    te_ctr_size: u8,

    // Legacy compatibility (kept for backward compatibility but using new names internally)
    /// Show timestamps (legacy flag, prefer -t for TCP)
    #[arg(long)]
    timestamps: bool,

    /// Verbose mode (legacy flag)
    #[arg(long)]
    verbose: bool,
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

    // Display license info if provided
    if let Some(license_file) = &args.license {
        println!("License file     : {}", license_file);
    } else {
        println!("License file     : NONE (assumed GPL)");
    }

    // Determine which input source to use
    if let Some(file_path) = args.file_input.clone() {
        // File input for postprocessing
        println!("File input       : {}", file_path);
        return run_file_input(&file_path, args).await;
    }

    if let Some(com_port) = args.com_port.clone() {
        // Serial port input
        let baud_rate = args.baud_rate;
        println!("Serial port      : {} @ {} baud", com_port, baud_rate);
        return run_serial_input(&com_port, baud_rate, args).await;
    }

    // Determine TCP or UDP (default is TCP on port 6601)
    let tcp_port = if let Some(port_opt) = args.tcp {
        port_opt.unwrap_or(6601)
    } else {
        6601  // Default TCP port
    };

    let udp_port = if let Some(port_opt) = args.udp {
        port_opt.unwrap_or(7701)
    } else {
        0  // UDP disabled by default
    };

    // Run TCP server (primary mode)
    let bind_addr = format!("0.0.0.0:{}", tcp_port);
    run_tcp_server(bind_addr, tcp_port, udp_port, args).await
}

async fn run_tcp_server(bind_addr: String, _tcp_port: u16, udp_port: u16, args: Args) -> Result<()> {
    println!("📡 Binding to TCP socket: {}", bind_addr.bright_cyan());
    
    let listener = TcpListener::bind(&bind_addr)
        .context(format!("Failed to bind TCP socket to {}", bind_addr))?;
    
    listener.set_nonblocking(true)
        .context("Failed to set non-blocking mode")?;

    println!("✓ TCP Server ready, waiting for connection...");
    
    // Show UDP status
    if udp_port > 0 {
        println!("📡 UDP socket: port {}", udp_port);
    }
    
    println!("  Press {} to stop\n", "Ctrl-C".bright_yellow());

    let mut parser = QSParser::new();
    let mut formatter = RecordFormatter::new(args.timestamps, false);

    let mut record_count: u64 = 0;
    let mut byte_count: u64 = 0;
    
    // State flags for keyboard commands (initialized from args)
    let mut quiet_mode = args.quiet.is_some();
    let mut screen_output = args.screen_output;
    let mut binary_output = args.binary_output;
    let mut matlab_output = args.matlab_output;
    let mut sequence_output = args.sequence_diagram.is_some();

    // Setup Ctrl-C handler
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl-C");
        tx.send(()).await.ok();
    });

    // Start keyboard listener (enabled by default unless -k flag is used)
    let keyboard_enabled = !args.no_keyboard;
    let keyboard_rx = keyboard::start_keyboard_listener(keyboard_enabled);
    
    if keyboard_enabled {
        println!("⌨️  Interactive keyboard control enabled");
        println!("   Press {} for help\n", "h".bright_yellow());
    }

    // Wait for connection
    let mut stream: Option<TcpStream> = None;
    
    loop {
        // Check for keyboard commands
        if let Ok(cmd) = keyboard_rx.try_recv() {
            match cmd {
                keyboard::KeyCommand::Exit => {
                    println!("\n\n{} Exiting...", "Exit command received.".bright_yellow());
                    break;
                }
                keyboard::KeyCommand::Help => {
                    keyboard::display_help();
                    keyboard::display_status(quiet_mode, screen_output, binary_output, 
                                           matlab_output, sequence_output);
                }
                keyboard::KeyCommand::Clear => {
                    if let Err(e) = keyboard::clear_screen() {
                        eprintln!("Failed to clear screen: {}", e);
                    }
                }
                keyboard::KeyCommand::ToggleQuiet => {
                    quiet_mode = !quiet_mode;
                    println!("🔇 Quiet mode: {}", 
                        if quiet_mode { "ON".bright_green() } else { "OFF".bright_red() });
                }
                keyboard::KeyCommand::Reset => {
                    if let Some(ref mut s) = stream {
                        if let Err(e) = commands::send_reset(s) {
                            eprintln!("Failed to send RESET: {}", e);
                        }
                    } else {
                        println!("⚠️  No target connected");
                    }
                }
                keyboard::KeyCommand::Info => {
                    if let Some(ref mut s) = stream {
                        if let Err(e) = commands::send_info(s) {
                            eprintln!("Failed to send INFO: {}", e);
                        }
                    } else {
                        println!("⚠️  No target connected");
                    }
                }
                keyboard::KeyCommand::Tick0 => {
                    if let Some(ref mut s) = stream {
                        if let Err(e) = commands::send_tick0(s) {
                            eprintln!("Failed to send TICK[0]: {}", e);
                        }
                    } else {
                        println!("⚠️  No target connected");
                    }
                }
                keyboard::KeyCommand::Tick1 => {
                    if let Some(ref mut s) = stream {
                        if let Err(e) = commands::send_tick1(s) {
                            eprintln!("Failed to send TICK[1]: {}", e);
                        }
                    } else {
                        println!("⚠️  No target connected");
                    }
                }
                keyboard::KeyCommand::SaveDictionaries => {
                    println!("💾 Saving dictionaries (not yet implemented)");
                }
                keyboard::KeyCommand::ToggleScreenOutput => {
                    screen_output = !screen_output;
                    println!("📺 Screen output: {}", 
                        if screen_output { "ON".bright_green() } else { "OFF".bright_red() });
                }
                keyboard::KeyCommand::ToggleBinaryOutput => {
                    binary_output = !binary_output;
                    println!("📄 Binary output: {}", 
                        if binary_output { "ON".bright_green() } else { "OFF".bright_red() });
                }
                keyboard::KeyCommand::ToggleMatlabOutput => {
                    matlab_output = !matlab_output;
                    println!("📊 Matlab output: {}", 
                        if matlab_output { "ON".bright_green() } else { "OFF".bright_red() });
                }
                keyboard::KeyCommand::ToggleSequenceOutput => {
                    sequence_output = !sequence_output;
                    println!("📋 Sequence output: {}", 
                        if sequence_output { "ON".bright_green() } else { "OFF".bright_red() });
                }
            }
        }
        
        // Check for Ctrl-C
        if rx.try_recv().is_ok() {
            println!("\n\n{} signal received, shutting down...", "Ctrl-C".bright_yellow());
            break;
        }

        // If no connection, try to accept one
        if stream.is_none() {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    print!("✓ Client connected from {}\r\n", addr.to_string().bright_green());
                    new_stream.set_read_timeout(Some(Duration::from_millis(100)))
                        .context("Failed to set read timeout")?;
                    stream = Some(new_stream);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No connection yet
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(e) => {
                    eprintln!("❌ Accept error: {}", e);
                    break;
                }
            }
        }

        // Read from connected stream
        if let Some(ref mut s) = stream {
            let mut buf = vec![0u8; 4096];
            match s.read(&mut buf) {
                Ok(0) => {
                    print!("⚠ Client disconnected\r\n");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    stream = None;
                    continue;
                }
                Ok(size) => {
                    byte_count += size as u64;
                    
                    if args.verbose {
                        println!("📦 Received {} bytes", size.to_string().bright_blue());
                    }

                    // Parse the data (may contain multiple frames)
                    if let Some(records) = parser.parse_packet(&buf[..size]) {
                        for record in records {
                            record_count += 1;
                            
                            // Update target configuration if received
                            formatter.set_config(parser.target_config().clone());
                            
                            // Check if this is a dictionary record
                            if let Some((_dict_type, dict_entry)) = parser::QSParser::parse_dictionary_record(&record, parser.target_config()) {
                                use parser::DictEntry;
                                
                                let dict = formatter.decoder_mut().dictionary_mut();
                                match dict_entry {
                                    DictEntry::Object(addr, name) => {
                                        raw_println!("           {:<12} 0x{:016X}->{}", "Obj-Dict", addr, name.bright_cyan());
                                        dict.objects.insert(addr, name);
                                    }
                                    DictEntry::Function(addr, name) => {
                                        raw_println!("           {:<12} 0x{:016X}->{}", "Fun-Dict", addr, name.bright_yellow());
                                        dict.functions.insert(addr, name);
                                    }
                                    DictEntry::Signal(sig, name) => {
                                        raw_println!("           {:<12} {:08X},Obj=0x0000000000000000->{}", "Sig-Dict", sig, name.bright_green());
                                        dict.signals.insert(addr, name);
                                    }
                                    DictEntry::UserRecord(rec_id, name) => {
                                        raw_println!("           {:<12} {:08X}->{}", "Usr-Dict", rec_id, name.bright_magenta());
                                        dict.user_records.insert(rec_id, name);
                                    }
                                }
                            } else {
                                // Only display if not in quiet mode
                                if !quiet_mode {
                                    formatter.format_record(&record);
                                }
                            }
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                    print!("⚠ Connection reset by peer\r\n");
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    stream = None;
                    continue;
                }
                Err(e) => {
                    eprintln!("❌ Read error: {}", e);
                    stream = None;
                    continue;
                }
            }
        }
    }

    // Print statistics
    let (frames_received, checksum_errors, sequence_gaps) = parser.stats();
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║                     Statistics                          ║");
    println!("╠════════════════════════════════════════════════════════╣");
    println!("║  Bytes received:    {:>10}                        ║", byte_count.to_string().bright_cyan());
    println!("║  Frames received:   {:>10}                        ║", frames_received.to_string().bright_cyan());
    println!("║  Records processed: {:>10}                        ║", record_count.to_string().bright_green());
    println!("║  Checksum errors:   {:>10}                        ║", checksum_errors.to_string().bright_red());
    println!("║  Sequence gaps:     {:>10}                        ║", sequence_gaps.to_string().bright_yellow());
    println!("╚════════════════════════════════════════════════════════╝\n");

    Ok(())
}

async fn run_udp_server(bind_addr: String, args: Args) -> Result<()> {
    println!("📡 Binding to UDP socket: {}", bind_addr.bright_cyan());
    
    let socket = UdpSocket::bind(&bind_addr)
        .context(format!("Failed to bind UDP socket to {}", bind_addr))?;
    
    socket.set_read_timeout(Some(Duration::from_secs(QSPY_TIMEOUT_SEC)))
        .context("Failed to set socket timeout")?;

    println!("✓ Socket ready, listening for QS traces...");
    println!("  Press {} to stop\n", "Ctrl-C".bright_yellow());

    let mut parser = QSParser::new();
    let mut formatter = RecordFormatter::new(args.timestamps, false);

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
    let (frames_received, checksum_errors, sequence_gaps) = parser.stats();
    println!("\n╔════════════════════════════════════════════════════════╗");
    println!("║                     Statistics                          ║");
    println!("╠════════════════════════════════════════════════════════╣");
    println!("║  Packets received:  {:>10}                        ║", packet_count.to_string().bright_cyan());
    println!("║  Frames received:   {:>10}                        ║", frames_received.to_string().bright_cyan());
    println!("║  Records processed: {:>10}                        ║", record_count.to_string().bright_green());
    println!("║  Checksum errors:   {:>10}                        ║", checksum_errors.to_string().bright_red());
    println!("║  Sequence gaps:     {:>10}                        ║", sequence_gaps.to_string().bright_yellow());
    println!("╚════════════════════════════════════════════════════════╝\n");

    Ok(())
}

// Stub function for serial port input (to be implemented)
async fn run_serial_input(_com_port: &str, _baud_rate: u32, _args: Args) -> Result<()> {
    eprintln!("❌ Serial port input not yet implemented");
    eprintln!("   Use -t for TCP/IP mode (default)");
    std::process::exit(1);
}

// Stub function for file input (to be implemented)
async fn run_file_input(_file_path: &str, _args: Args) -> Result<()> {
    eprintln!("❌ File input not yet implemented");
    eprintln!("   Use -t for TCP/IP mode (default)");
    std::process::exit(1);
}
