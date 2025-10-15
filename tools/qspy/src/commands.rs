//! QS-RX Commands (Host → Target)
//!
//! Bidirectional command protocol for interactive target control

use std::io::Write;

/// QS-RX Command Types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum QSRxCommand {
    /// Reset the target
    Reset = 0x00,
    /// Request target info
    Info = 0x01,
    /// Inject tick[0]
    Tick0 = 0x02,
    /// Inject tick[1]
    Tick1 = 0x03,
    /// Set global filter
    SetGlobalFilter = 0x04,
    /// Set local filter
    SetLocalFilter = 0x05,
    /// Peek memory
    Peek = 0x06,
    /// Poke memory
    Poke = 0x07,
    /// Execute user command
    UserCommand = 0x08,
    /// Test probe
    TestProbe = 0x09,
    /// Dispatch event
    EventDispatch = 0x0A,
    /// Post event
    EventPost = 0x0B,
    /// Publish event
    EventPublish = 0x0C,
}

impl QSRxCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Reset => "RESET",
            Self::Info => "INFO",
            Self::Tick0 => "TICK[0]",
            Self::Tick1 => "TICK[1]",
            Self::SetGlobalFilter => "SET_GLOBAL_FILTER",
            Self::SetLocalFilter => "SET_LOCAL_FILTER",
            Self::Peek => "PEEK",
            Self::Poke => "POKE",
            Self::UserCommand => "USER_CMD",
            Self::TestProbe => "TEST_PROBE",
            Self::EventDispatch => "EVENT_DISPATCH",
            Self::EventPost => "EVENT_POST",
            Self::EventPublish => "EVENT_PUBLISH",
        }
    }
}

/// Add byte with HDLC stuffing
fn add_stuffed_byte(frame: &mut Vec<u8>, byte: u8) {
    const FLAG: u8 = 0x7E;
    const ESC: u8 = 0x7D;
    const ESC_XOR: u8 = 0x20;
    
    if byte == FLAG || byte == ESC {
        frame.push(ESC);
        frame.push(byte ^ ESC_XOR);
    } else {
        frame.push(byte);
    }
}

/// Build and send a QS-RX command with HDLC framing
pub fn send_command<W: Write>(writer: &mut W, cmd: QSRxCommand, data: &[u8]) -> std::io::Result<()> {
    // HDLC frame format: FLAG | SEQ | CMD | DATA | CHECKSUM | FLAG
    const FLAG: u8 = 0x7E;
    
    let mut frame = Vec::new();
    let sequence = 0u8; // For simplicity, using 0 sequence number for commands
    
    // Calculate checksum: ~(seq + cmd + sum(data))
    let mut checksum: u8 = sequence;
    checksum = checksum.wrapping_add(cmd as u8);
    for &byte in data {
        checksum = checksum.wrapping_add(byte);
    }
    checksum = !checksum;
    
    // Build frame
    frame.push(FLAG);
    add_stuffed_byte(&mut frame, sequence);
    add_stuffed_byte(&mut frame, cmd as u8);
    for &byte in data {
        add_stuffed_byte(&mut frame, byte);
    }
    add_stuffed_byte(&mut frame, checksum);
    frame.push(FLAG);
    
    // Send frame
    writer.write_all(&frame)?;
    writer.flush()?;
    
    Ok(())
}

/// Send RESET command to target
pub fn send_reset<W: Write>(writer: &mut W) -> std::io::Result<()> {
    println!("→ Sending RESET command to target");
    send_command(writer, QSRxCommand::Reset, &[])
}

/// Send INFO command to request target configuration
pub fn send_info<W: Write>(writer: &mut W) -> std::io::Result<()> {
    println!("→ Sending INFO command to target");
    send_command(writer, QSRxCommand::Info, &[])
}

/// Send TICK[0] command
pub fn send_tick0<W: Write>(writer: &mut W) -> std::io::Result<()> {
    println!("→ Sending TICK[0] command to target");
    send_command(writer, QSRxCommand::Tick0, &[])
}

/// Send TICK[1] command
pub fn send_tick1<W: Write>(writer: &mut W) -> std::io::Result<()> {
    println!("→ Sending TICK[1] command to target");
    send_command(writer, QSRxCommand::Tick1, &[])
}

/// Send global filter update
pub fn send_global_filter<W: Write>(writer: &mut W, filter: u128) -> std::io::Result<()> {
    let bytes = filter.to_le_bytes();
    println!("→ Sending GLOBAL_FILTER command to target: 0x{:032x}", filter);
    send_command(writer, QSRxCommand::SetGlobalFilter, &bytes)
}

/// Send local filter update
pub fn send_local_filter<W: Write>(writer: &mut W, filter: u128) -> std::io::Result<()> {
    let bytes = filter.to_le_bytes();
    println!("→ Sending LOCAL_FILTER command to target: 0x{:032x}", filter);
    send_command(writer, QSRxCommand::SetLocalFilter, &bytes)
}

/// Send peek command to read memory
pub fn send_peek<W: Write>(writer: &mut W, address: u32, size: u8) -> std::io::Result<()> {
    let mut data = Vec::new();
    data.extend_from_slice(&address.to_le_bytes());
    data.push(size);
    println!("→ Sending PEEK command: address=0x{:08x}, size={}", address, size);
    send_command(writer, QSRxCommand::Peek, &data)
}

/// Send poke command to write memory
pub fn send_poke<W: Write>(writer: &mut W, address: u32, data: &[u8]) -> std::io::Result<()> {
    let mut cmd_data = Vec::new();
    cmd_data.extend_from_slice(&address.to_le_bytes());
    cmd_data.push(data.len() as u8);
    cmd_data.extend_from_slice(data);
    println!("→ Sending POKE command: address=0x{:08x}, size={}", address, data.len());
    send_command(writer, QSRxCommand::Poke, &cmd_data)
}
