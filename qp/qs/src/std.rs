//! QS - Software Tracing for std environments
//!
//! Provides tracing output to stdout or UDP (for QSpy host tool).

use std::sync::Mutex;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::net::UdpSocket;

/// Trace record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum QSRecordType {
    QS_SM_INIT = 0,
    QS_SM_DISPATCH,
    QS_SM_STATE_ENTRY,
    QS_SM_STATE_EXIT,
    QS_SM_TRAN,
    QS_QF_POST,
    QS_QF_PUBLISH,
    QS_QF_ACTIVE_INIT,
    QS_QF_TICK,
    QS_QF_TIMEEVT_ARM,
    QS_QF_TIMEEVT_DISARM,
    QS_QF_TIMEEVT_POST,
    QS_QF_MPOOL_GET,
    QS_QF_MPOOL_PUT,
    QS_USER = 100,
}

impl QSRecordType {
    pub fn name(&self) -> &'static str {
        match self {
            QSRecordType::QS_SM_INIT => "SM_INIT",
            QSRecordType::QS_SM_DISPATCH => "SM_DISPATCH",
            QSRecordType::QS_SM_STATE_ENTRY => "SM_ENTRY",
            QSRecordType::QS_SM_STATE_EXIT => "SM_EXIT",
            QSRecordType::QS_SM_TRAN => "SM_TRAN",
            QSRecordType::QS_QF_POST => "QF_POST",
            QSRecordType::QS_QF_PUBLISH => "QF_PUBLISH",
            QSRecordType::QS_QF_ACTIVE_INIT => "AO_INIT",
            QSRecordType::QS_QF_TICK => "TICK",
            QSRecordType::QS_QF_TIMEEVT_ARM => "TE_ARM",
            QSRecordType::QS_QF_TIMEEVT_DISARM => "TE_DISARM",
            QSRecordType::QS_QF_TIMEEVT_POST => "TE_POST",
            QSRecordType::QS_QF_MPOOL_GET => "MP_GET",
            QSRecordType::QS_QF_MPOOL_PUT => "MP_PUT",
            QSRecordType::QS_USER => "USER",
        }
    }
}

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

    pub fn begin(&mut self, record_type: QSRecordType) -> bool {
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

    pub fn flush(&mut self) -> io::Result<()> {
        match self.output_mode {
            QSOutputMode::Stdout => self.flush_stdout(),
            QSOutputMode::Udp => self.flush_udp(),
        }
    }
    
    fn flush_stdout(&mut self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        
        while let Some(record) = self.records.pop_front() {
            write!(handle, "[QS:{:08}] {:12} ", record.timestamp, record.record_type.name())?;
            
            // Format data based on record type
            match record.record_type {
                QSRecordType::QS_SM_INIT | 
                QSRecordType::QS_SM_DISPATCH |
                QSRecordType::QS_SM_TRAN => {
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
                // Build UDP packet: seq_num + record_type + data
                let mut packet = Vec::with_capacity(record.data.len() + 2);
                packet.push(self.sequence);
                packet.push(record.record_type as u8);
                packet.extend_from_slice(&record.data);
                
                // Send to QSpy
                match socket.send_to(&packet, &self.qspy_addr) {
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
}

static QS_BUF: Mutex<QSBuffer> = Mutex::new(QSBuffer::new());

pub fn init() {
    QS_BUF.lock().unwrap().init();
}

/// Initialize QS to send traces to QSpy host tool via UDP
pub fn init_udp(host: &str, port: u16) -> io::Result<()> {
    QS_BUF.lock().unwrap().init_udp(host, port)
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

pub fn begin(record_type: QSRecordType) -> bool {
    QS_BUF.lock().unwrap().begin(record_type)
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

pub fn end() {
    QS_BUF.lock().unwrap().end();
}

pub fn flush() -> io::Result<()> {
    QS_BUF.lock().unwrap().flush()
}

// Macros for tracing
#[macro_export]
macro_rules! qs_sm_init {
    ($obj:expr, $state:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_INIT) {
            $crate::u32($obj as *const _ as u32);
            $crate::u32($state as usize as u32);
            $crate::end();
        }
    };
}

#[macro_export]
macro_rules! qs_sm_dispatch {
    ($obj:expr, $signal:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_DISPATCH) {
            $crate::u32($obj as *const _ as u32);
            $crate::u16($signal);
            $crate::end();
        }
    };
}

#[macro_export]
macro_rules! qs_sm_tran {
    ($obj:expr, $source:expr, $target:expr) => {
        if $crate::begin($crate::QSRecordType::QS_SM_TRAN) {
            // Use usize for pointer addresses on 64-bit systems
            $crate::u32(($obj as *const _ as *const () as usize) as u32);
            $crate::u32($source as usize as u32);
            $crate::u32($target as usize as u32);
            $crate::end();
        }
    };
}
