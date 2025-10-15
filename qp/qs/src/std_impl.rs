//! QS - Software Tracing for std environments
//!
//! Provides tracing output to stdout or UDP (for QSpy host tool).

use std::sync::Mutex;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::net::{UdpSocket, TcpStream};
use crate::types::QSRecordType;

/// QS trace record
pub struct QSRecord {
    pub record_type: QSRecordType,
    pub timestamp: u64,
    pub data: Vec<u8>,
}

/// QS output mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QSOutputMode {
    /// Output to stdout (default)
    Stdout,
    /// Output to UDP socket for QSpy host tool
    Udp,
    /// Output to TCP socket for QSpy host tool
    Tcp,
}

/// QS trace buffer for std
pub struct QSBuffer {
    records: VecDeque<QSRecord>,
    filter: u128,
    enabled: bool,
    timestamp: u64,
    current_record: Option<(QSRecordType, Vec<u8>)>,
    output_mode: QSOutputMode,
    udp_socket: Option<UdpSocket>,
    tcp_stream: Option<TcpStream>,
    qspy_addr: String,
    sequence: u8,
}

impl QSBuffer {
    const fn new() -> Self {
        Self {
            records: VecDeque::new(),
            filter: u128::MAX,
            enabled: false,
            timestamp: 0,
            current_record: None,
            output_mode: QSOutputMode::Stdout,
            udp_socket: None,
            tcp_stream: None,
            qspy_addr: String::new(),
            sequence: 0,
        }
    }

    pub fn init(&mut self) {
        self.records.clear();
        self.filter = u128::MAX;
        self.enabled = true;
        self.timestamp = 0;
    }
    
    pub fn init_udp(&mut self, qspy_host: &str, qspy_port: u16) -> io::Result<()> {
        self.records.clear();
        self.filter = u128::MAX;
        self.enabled = true;
        self.timestamp = 0;
        self.output_mode = QSOutputMode::Udp;
        self.qspy_addr = format!("{}:{}", qspy_host, qspy_port);
        
        // Create UDP socket
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        self.udp_socket = Some(socket);
        
        println!("QS: UDP output initialized to {}", self.qspy_addr);
        Ok(())
    }

    pub fn init_tcp(&mut self, qspy_host: &str, qspy_port: u16) -> io::Result<()> {
        self.records.clear();
        self.filter = u128::MAX;
        self.enabled = true;
        self.timestamp = 0;
        self.output_mode = QSOutputMode::Tcp;
        self.qspy_addr = format!("{}:{}", qspy_host, qspy_port);
        
        // Connect to QSpy TCP server
        let stream = TcpStream::connect(&self.qspy_addr)?;
        stream.set_nonblocking(true)?;
        self.tcp_stream = Some(stream);
        
        println!("QS: TCP output initialized to {}", self.qspy_addr);
        Ok(())
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_filter(&mut self, record_type: QSRecordType, enable: bool) {
        let bit = record_type as u8;
        if enable {
            self.filter |= 1u128 << bit;
        } else {
            self.filter &= !(1u128 << bit);
        }
    }

    pub fn is_filtered(&self, record_type: QSRecordType) -> bool {
        let bit = record_type as u8;
        (self.filter & (1u128 << bit)) != 0
    }

    pub fn begin(&mut self, record_type: QSRecordType, _qs_id: u8) -> bool {
        if !self.enabled || !self.is_filtered(record_type) {
            return false;
        }
        
        self.timestamp += 1;
        self.current_record = Some((record_type, Vec::new()));
        true
    }

    pub fn u8(&mut self, value: u8) {
        if let Some((_, ref mut data)) = self.current_record {
            data.push(value);
        }
    }

    pub fn u16(&mut self, value: u16) {
        if let Some((_, ref mut data)) = self.current_record {
            data.extend_from_slice(&value.to_le_bytes());
        }
    }

    pub fn u32(&mut self, value: u32) {
        if let Some((_, ref mut data)) = self.current_record {
            data.extend_from_slice(&value.to_le_bytes());
        }
    }

    pub fn str(&mut self, value: &str) {
        if let Some((_, ref mut data)) = self.current_record {
            data.extend_from_slice(value.as_bytes());
            data.push(0); // null terminator
        }
    }

    pub fn end(&mut self) {
        if let Some((record_type, data)) = self.current_record.take() {
            let record = QSRecord {
                record_type,
                timestamp: self.timestamp,
                data,
            };
            self.records.push_back(record);
        }
    }

    pub fn target_info(&mut self, qp_version: &str, target_name: &str, _endianness: u8) {
        let mut data = Vec::new();
        
        // QP/C 8.1.1 TARGET_INFO format:
        // 1. Info byte (1 byte) - bit 7: endianness (0x80 = big-endian, 0x00 = little-endian)
        // 2. QP_RELEASE (4 bytes, u32) - version as 0x00MMNNPP (major, minor, patch)
        // 3. Packed sizes (5 bytes) - two 4-bit sizes per byte
        // 4. Bounds (2 bytes) - MAX_ACTIVE, MAX_EPOOL|MAX_TICK_RATE
        // 5. Build time (3 bytes) - seconds, minutes, hours (decimal BCD)
        // 6. Build date (3 bytes) - day, month, year%100 (decimal BCD)
        // 7. Target name (optional, null-terminated string)
        
        // 1. Info byte with flags
        // bit 0: reserved (0)
        // bit 1: new format flag (0x02)
        // bit 2-3: QP framework (0x04=QP/C, 0x08=QP/C++)
        // bit 6: reset flag (0x00 for info, 0x40 for reset)
        // bit 7: endianness (0x00=little, 0x80=big)
        let mut info = 0x02u8; // New format
        info |= 0x04; // QP/C framework
        if cfg!(target_endian = "big") {
            info |= 0x80; // Big-endian
        }
        data.push(info);
        
        // 2. Parse version string (e.g., "8.1.0" or "QP-Rust 8.1.0") to u32
        let version_u32 = if let Some(ver_part) = qp_version.split_whitespace().last() {
            let parts: Vec<&str> = ver_part.split('.').collect();
            if parts.len() >= 3 {
                let major = parts[0].parse::<u32>().unwrap_or(8);
                let minor = parts[1].parse::<u32>().unwrap_or(1);
                let patch = parts[2].parse::<u32>().unwrap_or(0);
                // Format: major*100 + minor*10 + patch (e.g., 810 for 8.1.0)
                // Combined with build date in upper bits, but we'll keep date as 0
                major * 100 + minor * 10 + patch
            } else {
                810 // Default to 8.1.0
            }
        } else {
            810 // Default to 8.1.0
        };
        // QSPY applies bitwise NOT when reading, so we send ~version
        let version_inverted = !version_u32;
        data.extend_from_slice(&version_inverted.to_le_bytes());
        
        // 3. Configuration data (13 bytes total):
        //    - 5 bytes: packed sizes
        //    - 2 bytes: bounds
        //    - 6 bytes: build timestamp (displayed as YYMMDD_HHMMSS)
        
        // First 5 bytes: packed sizes (nibble-packed)
        // NOTE: C code hardcodes signal to 2!
        let signal_size = 2u8;  // HARDCODED in QP/C
        let event_size = 2u8;
        let queue_ctr_size = 2u8;
        let time_evt_ctr_size = 2u8;
        let pool_blk_size = 2u8;
        let pool_ctr_size = 2u8;
        // WORKAROUND: Use 4-byte pointers even on 64-bit to match QP/C behavior
        // QP/C truncates pointers to 4 bytes for tracing to save bandwidth
        let obj_ptr_size = 4u8;  // Always 4 bytes for compatibility
        let fun_ptr_size = 4u8;  // Always 4 bytes for compatibility
        let time_size = 4u8;
        
        data.push(signal_size | (event_size << 4));              // buf[0]
        data.push(queue_ctr_size | (time_evt_ctr_size << 4));    // buf[1]
        data.push(pool_blk_size | (pool_ctr_size << 4));         // buf[2]
        data.push(obj_ptr_size | (fun_ptr_size << 4));           // buf[3]
        data.push(time_size);                                     // buf[4]
        
        // Next 2 bytes: bounds
        data.push(5);    // buf[5]: QF_MAX_ACTIVE
        data.push(0x12); // buf[6]: QF_MAX_EPOOL=2 | QF_MAX_TICK_RATE=1
        
        // Last 6 bytes: build timestamp - displayed as YYMMDD_HHMMSS
        // QSPY reads: tbuild[5][4][3][2][1][0] and displays as:
        // YY=tbuild[5], MM=tbuild[4], DD=tbuild[3], HH=tbuild[2], MM=tbuild[1], SS=tbuild[0]
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = now.as_secs();
        let sec = (secs % 60) as u8;
        let min = ((secs / 60) % 60) as u8;
        let hour = ((secs / 3600) % 24) as u8;
        
        data.push(sec);   // buf[7] = tbuild[0] = SS (seconds)
        data.push(min);   // buf[8] = tbuild[1] = MM (minutes)
        data.push(hour);  // buf[9] = tbuild[2] = HH (hours)
        data.push(15);    // buf[10] = tbuild[3] = DD (day)
        data.push(10);    // buf[11] = tbuild[4] = MM (month)
        data.push(25);    // buf[12] = tbuild[5] = YY (year % 100)
        
        // NOTE: QP/C does NOT send target name in TARGET_INFO record!
        // The record ends here at exactly 18 bytes.
        
        // Create record with special TARGET_INFO type (64)
        // Note: Using hardcoded value since QSRecordType in std_impl doesn't have TARGET_INFO
        let record = QSRecord {
            record_type: unsafe { std::mem::transmute::<u8, QSRecordType>(64) },
            timestamp: 0, // No timestamp for target info
            data,
        };
        
        eprintln!("QS: Generated TARGET_INFO record ({} bytes)", record.data.len());
        self.records.push_back(record);
    }

    pub fn flush(&mut self) -> io::Result<()> {
        match self.output_mode {
            QSOutputMode::Stdout => self.flush_stdout(),
            QSOutputMode::Udp => self.flush_udp(),
            QSOutputMode::Tcp => self.flush_tcp(),
        }
    }
    
    fn flush_stdout(&mut self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        
        while let Some(record) = self.records.pop_front() {
            write!(handle, "[QS:{:08}] {:12} ", record.timestamp, record.record_type.name())?;
            
            // Format data based on record type
            match record.record_type {
                QSRecordType::QS_QEP_STATE_INIT | 
                QSRecordType::QS_QEP_DISPATCH |
                QSRecordType::QS_QEP_TRAN => {
                    if record.data.len() >= 4 {
                        let obj_id = u32::from_le_bytes([
                            record.data[0], record.data[1], 
                            record.data[2], record.data[3]
                        ]);
                        write!(handle, "obj={:08x}", obj_id)?;
                        
                        if record.data.len() >= 8 {
                            let state = u32::from_le_bytes([
                                record.data[4], record.data[5],
                                record.data[6], record.data[7]
                            ]);
                            write!(handle, " state={:08x}", state)?;
                        }
                        
                        if record.data.len() >= 12 {
                            let target = u32::from_le_bytes([
                                record.data[8], record.data[9],
                                record.data[10], record.data[11]
                            ]);
                            write!(handle, " target={:08x}", target)?;
                        }
                    }
                }
                _ => {
                    // Generic hex dump for other records
                    for byte in &record.data {
                        write!(handle, "{:02x}", byte)?;
                    }
                }
            }
            writeln!(handle)?;
        }
        
        handle.flush()
    }
    
    fn flush_udp(&mut self) -> io::Result<()> {
        if let Some(ref socket) = self.udp_socket {
            while let Some(record) = self.records.pop_front() {
                // Build HDLC frame: sequence + record_type + timestamp + data + checksum + flag
                // Note: Dictionary and TARGET_INFO records (54-64) don't have timestamps
                let record_type_u8 = record.record_type as u8;
                let has_timestamp = record_type_u8 < 54 || record_type_u8 > 64;
                let timestamp = if has_timestamp { record.timestamp } else { 0 };
                let frame = self.build_hdlc_frame(record_type_u8, timestamp, &record.data, !has_timestamp);
                
                // Send to QSpy
                match socket.send_to(&frame, &self.qspy_addr) {
                    Ok(_) => {
                        self.sequence = self.sequence.wrapping_add(1);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Socket not ready, push record back and try later
                        self.records.push_front(record);
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Ok(())
    }

    fn flush_tcp(&mut self) -> io::Result<()> {
        if self.tcp_stream.is_some() {
            while let Some(record) = self.records.pop_front() {
                // Build HDLC frame: sequence + record_type + timestamp + data + checksum + flag
                // Note: Dictionary and TARGET_INFO records (54-64) don't have timestamps
                let record_type_u8 = record.record_type as u8;
                let has_timestamp = record_type_u8 < 54 || record_type_u8 > 64;
                let timestamp = if has_timestamp { record.timestamp } else { 0 };
                let frame = self.build_hdlc_frame(record_type_u8, timestamp, &record.data, !has_timestamp);
                
                // Send to QSpy via TCP
                let stream = self.tcp_stream.as_mut().unwrap();
                match stream.write_all(&frame) {
                    Ok(_) => {
                        self.sequence = self.sequence.wrapping_add(1);
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        // Socket not ready, push record back and try later
                        self.records.push_front(record);
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                }
            }
            if let Some(stream) = self.tcp_stream.as_mut() {
                stream.flush()?;
            }
        }
        Ok(())
    }

    /// Build HDLC frame with byte-stuffing, checksum, and flag
    /// Frame format: [sequence][record_type][timestamp?][data...][checksum][0x7E]
    /// Dictionary and TARGET_INFO records (64-76) omit the timestamp field
    fn build_hdlc_frame(&self, record_type: u8, timestamp: u64, data: &[u8], skip_timestamp: bool) -> Vec<u8> {
        const HDLC_FLAG: u8 = 0x7E;
        const HDLC_ESC: u8 = 0x7D;
        const ESC_XOR: u8 = 0x20;

        // Timestamp as 4 bytes (default for std environments) - only for non-dictionary records
        let ts_bytes = if !skip_timestamp {
            (timestamp as u32).to_le_bytes()
        } else {
            [0u8; 4] // Will not be used
        };

        // Calculate checksum: ~(sequence + record_type + [timestamp] + sum(data))
        let mut checksum: u8 = self.sequence;
        checksum = checksum.wrapping_add(record_type);
        if !skip_timestamp {
            for &byte in &ts_bytes {
                checksum = checksum.wrapping_add(byte);
            }
        }
        for &byte in data {
            checksum = checksum.wrapping_add(byte);
        }
        checksum = !checksum;

        // Build frame with byte-stuffing
        let mut frame = Vec::with_capacity((data.len() + 8) * 2); // Estimate with stuffing

        // Helper closure to add byte with stuffing
        let mut add_byte = |frame: &mut Vec<u8>, byte: u8| {
            if byte == HDLC_FLAG || byte == HDLC_ESC {
                frame.push(HDLC_ESC);
                frame.push(byte ^ ESC_XOR);
            } else {
                frame.push(byte);
            }
        };

        // Add sequence number (with stuffing)
        add_byte(&mut frame, self.sequence);

        // Add record type (with stuffing)
        add_byte(&mut frame, record_type);

        // Add timestamp bytes (with stuffing) - only for non-dictionary records
        if !skip_timestamp {
            for &byte in &ts_bytes {
                add_byte(&mut frame, byte);
            }
        }

        // Add data bytes (with stuffing)
        for &byte in data {
            add_byte(&mut frame, byte);
        }

        // Add checksum (with stuffing)
        add_byte(&mut frame, checksum);

        // Add flag delimiter (never stuffed)
        frame.push(HDLC_FLAG);

        frame
    }
}

static QS_BUF: Mutex<QSBuffer> = Mutex::new(QSBuffer::new());

pub fn init() {
    QS_BUF.lock().unwrap().init();
}

/// Initialize QS to send traces to QSpy host tool via UDP
pub fn init_udp(host: &str, port: u16) -> io::Result<()> {
    QS_BUF.lock().unwrap().init_udp(host, port)
}

/// Initialize QS to send traces to QSpy host tool via TCP
pub fn init_tcp(host: &str, port: u16) -> io::Result<()> {
    QS_BUF.lock().unwrap().init_tcp(host, port)
}

pub fn enable() {
    QS_BUF.lock().unwrap().enable();
}

pub fn disable() {
    QS_BUF.lock().unwrap().disable();
}

pub fn is_enabled() -> bool {
    QS_BUF.lock().unwrap().is_enabled()
}

pub fn set_filter(record_type: QSRecordType, enable: bool) {
    QS_BUF.lock().unwrap().set_filter(record_type, enable);
}

pub fn begin(record_type: QSRecordType, qs_id: u8) -> bool {
    QS_BUF.lock().unwrap().begin(record_type, qs_id)
}

pub fn u8(value: u8) {
    QS_BUF.lock().unwrap().u8(value);
}

pub fn u16(value: u16) {
    QS_BUF.lock().unwrap().u16(value);
}

pub fn u32(value: u32) {
    QS_BUF.lock().unwrap().u32(value);
}

pub fn str(value: &str) {
    QS_BUF.lock().unwrap().str(value);
}

pub fn obj_ptr(ptr: usize) {
    // WORKAROUND: Always output as u32 (4 bytes) to match QP/C behavior
    // QP/C truncates pointers to 4 bytes for tracing to save bandwidth
    QS_BUF.lock().unwrap().u32(ptr as u32);
}

pub fn fun_ptr(ptr: usize) {
    // WORKAROUND: Always output as u32 (4 bytes) to match QP/C behavior
    // QP/C truncates pointers to 4 bytes for tracing to save bandwidth
    QS_BUF.lock().unwrap().u32(ptr as u32);
}

pub fn end() {
    QS_BUF.lock().unwrap().end();
}

pub fn target_info(qp_version: &str, target_name: &str, endianness: u8) {
    QS_BUF.lock().unwrap().target_info(qp_version, target_name, endianness);
}

pub fn flush() -> io::Result<()> {
    QS_BUF.lock().unwrap().flush()
}
