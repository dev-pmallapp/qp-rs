//! QS Packet Parser
//!
//! Parses HDLC-framed QS trace records according to QP/Spy protocol specification
//! 
//! Frame structure: [Sequence][RecordType][Data...][Checksum][Flag 0x7E]
//! 
//! Features:
//! - HDLC frame parsing with 0x7E flag delimiter
//! - Byte unstuffing (transparency) for 0x7D and 0x7E bytes
//! - Checksum validation
//! - Frame synchronization and error recovery

use crate::protocol::{QSRecord, QSRecordType, TargetConfig};

/// Dictionary entry types
pub enum DictEntry {
    Object(u64, String),
    Function(u64, String),
    Signal(u16, String),
    UserRecord(u8, String),
}

/// HDLC frame flag byte
const HDLC_FLAG: u8 = 0x7E;

/// HDLC escape byte
const HDLC_ESC: u8 = 0x7D;

/// XOR mask for escaped bytes
const ESC_XOR: u8 = 0x20;

/// Parser state machine
#[derive(Debug, Clone, Copy, PartialEq)]
enum ParserState {
    /// Waiting for data after flag
    WaitingForData,
    /// Collecting data bytes
    CollectingData,
    /// Got checksum, waiting for flag
    GotChecksum,
}

pub struct QSParser {
    /// Current parser state
    state: ParserState,
    /// Frame sequence number
    sequence: u8,
    /// Expected next sequence number
    expected_sequence: u8,
    /// Record type byte
    record_type: u8,
    /// Accumulated data buffer
    data_buffer: Vec<u8>,
    /// Checksum byte
    checksum: u8,
    /// Escape flag for next byte
    escape_next: bool,
    /// Timestamp counter
    timestamp: u64,
    /// Target configuration (received from QS_TARGET_INFO)
    target_config: TargetConfig,
    /// Statistics
    frames_received: u64,
    checksum_errors: u64,
    sequence_gaps: u64,
}

impl QSParser {
    pub fn new() -> Self {
        Self {
            state: ParserState::WaitingForData,
            sequence: 0,
            expected_sequence: 0,
            record_type: 0,
            data_buffer: Vec::with_capacity(256),
            checksum: 0,
            escape_next: false,
            timestamp: 0,
            target_config: TargetConfig::default(),
            frames_received: 0,
            checksum_errors: 0,
            sequence_gaps: 0,
        }
    }
    
    /// Get current target configuration
    pub fn target_config(&self) -> &TargetConfig {
        &self.target_config
    }
    
    /// Parse dictionary record and return key-value pair
    pub fn parse_dictionary_record(record: &QSRecord, config: &TargetConfig) -> Option<(QSRecordType, DictEntry)> {
        use QSRecordType::*;
        
        match record.record_type {
            QS_OBJ_DICT => {
                // Format: OBJ_PTR + STRING
                let (addr, name) = Self::parse_ptr_and_string(&record.data, config.obj_ptr_size)?;
                Some((QS_OBJ_DICT, DictEntry::Object(addr, name)))
            }
            QS_FUN_DICT => {
                // Format: FUN_PTR + STRING
                let (addr, name) = Self::parse_ptr_and_string(&record.data, config.fun_ptr_size)?;
                Some((QS_FUN_DICT, DictEntry::Function(addr, name)))
            }
            QS_SIG_DICT => {
                // Format: SIGNAL + OBJ_PTR + STRING
                if record.data.len() < config.signal_size as usize + config.obj_ptr_size as usize {
                    return None;
                }
                
                let sig = match config.signal_size {
                    1 => record.data[0] as u16,
                    2 => u16::from_le_bytes([record.data[0], record.data[1]]),
                    _ => u16::from_le_bytes([record.data[0], record.data[1]]),
                };
                
                let name_start = config.signal_size as usize + config.obj_ptr_size as usize;
                if let Some(null_pos) = record.data[name_start..].iter().position(|&b| b == 0) {
                    if let Ok(name) = std::str::from_utf8(&record.data[name_start..name_start + null_pos]) {
                        return Some((QS_SIG_DICT, DictEntry::Signal(sig, name.to_string())));
                    }
                }
                None
            }
            QS_USR_DICT => {
                // Format: U8 + STRING
                if record.data.is_empty() {
                    return None;
                }
                
                let rec_id = record.data[0];
                if let Some(null_pos) = record.data[1..].iter().position(|&b| b == 0) {
                    if let Ok(name) = std::str::from_utf8(&record.data[1..1 + null_pos]) {
                        return Some((QS_USR_DICT, DictEntry::UserRecord(rec_id, name.to_string())));
                    }
                }
                None
            }
            _ => None,
        }
    }
    
    fn parse_ptr_and_string(data: &[u8], ptr_size: u8) -> Option<(u64, String)> {
        if data.len() < ptr_size as usize {
            return None;
        }
        
        let addr = match ptr_size {
            1 => data[0] as u64,
            2 => u16::from_le_bytes([data[0], data[1]]) as u64,
            4 => u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as u64,
            8 => u64::from_le_bytes([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]),
            _ => return None,
        };
        
        if let Some(null_pos) = data[ptr_size as usize..].iter().position(|&b| b == 0) {
            if let Ok(name) = std::str::from_utf8(&data[ptr_size as usize..ptr_size as usize + null_pos]) {
                return Some((addr, name.to_string()));
            }
        }
        
        None
    }

    /// Parse incoming byte stream containing HDLC frames
    /// Returns vector of successfully parsed records
    pub fn parse_packet(&mut self, data: &[u8]) -> Option<Vec<QSRecord>> {
        let mut records = Vec::new();

        for &byte in data {
            if let Some(record) = self.process_byte(byte) {
                records.push(record);
            }
        }

        if records.is_empty() {
            None
        } else {
            Some(records)
        }
    }

    /// Process a single byte from the stream
    /// Returns a complete record when frame is complete and valid
    fn process_byte(&mut self, byte: u8) -> Option<QSRecord> {
        // Flag byte always starts new frame
        if byte == HDLC_FLAG && !self.escape_next {
            return self.handle_flag();
        }

        // Handle escape sequences
        if let Some(unstuffed) = self.unstuff_byte(byte) {
            self.feed_byte(unstuffed)
        } else {
            None
        }
    }

    /// Handle HDLC flag byte (0x7E)
    fn handle_flag(&mut self) -> Option<QSRecord> {
        // If we were collecting data, this flag terminates the frame
        if matches!(self.state, ParserState::CollectingData | ParserState::GotChecksum) {
            // Validate and extract record
            let record = self.complete_frame();
            self.reset_frame();
            record
        } else {
            // Flag at start, just reset
            self.reset_frame();
            None
        }
    }

    /// Unstuff (unescape) a byte according to HDLC transparency rules
    /// Returns None if this is an escape byte (0x7D), Some(byte) otherwise
    fn unstuff_byte(&mut self, byte: u8) -> Option<u8> {
        if self.escape_next {
            self.escape_next = false;
            Some(byte ^ ESC_XOR)
        } else if byte == HDLC_ESC {
            self.escape_next = true;
            None
        } else {
            Some(byte)
        }
    }

    /// Feed an unstuffed byte to the frame parser
    fn feed_byte(&mut self, byte: u8) -> Option<QSRecord> {
        match self.state {
            ParserState::WaitingForData => {
                // First byte is sequence number
                self.sequence = byte;
                self.state = ParserState::CollectingData;
                None
            }
            ParserState::CollectingData => {
                if self.record_type == 0 {
                    // Second byte is record type
                    self.record_type = byte;
                    None
                } else {
                    // Subsequent bytes are data, last byte before flag is checksum
                    // We don't know if this is data or checksum until we see the flag
                    // So we accumulate everything and validate on flag
                    self.data_buffer.push(byte);
                    None
                }
            }
            ParserState::GotChecksum => {
                // Shouldn't reach here - flag should come after checksum
                None
            }
        }
    }

    /// Complete a frame when flag is encountered
    /// Validates checksum and sequence number, returns record if valid
    fn complete_frame(&mut self) -> Option<QSRecord> {
        // Need at least checksum (last byte in data_buffer)
        if self.data_buffer.is_empty() {
            return None;
        }

        // Last byte is checksum
        self.checksum = self.data_buffer.pop().unwrap();
        
        // Validate checksum
        if !self.validate_checksum() {
            self.checksum_errors += 1;
            eprintln!("<COMMS> ERROR    Checksum mismatch in frame");
            return None;
        }

        // Check sequence number
        if self.sequence != self.expected_sequence {
            if self.frames_received > 0 {
                let gap = if self.sequence > self.expected_sequence {
                    self.sequence - self.expected_sequence
                } else {
                    // Sequence wrapped around
                    (256 - self.expected_sequence as u16 + self.sequence as u16) as u8
                };
                eprintln!("<COMMS> ERROR    Sequence gap detected: expected {}, got {} (gap={})",
                         self.expected_sequence, self.sequence, gap);
                self.sequence_gaps += 1;
            }
        }

        // Update expected sequence
        self.expected_sequence = self.sequence.wrapping_add(1);
        self.frames_received += 1;

        // Parse record type
        let record_type = QSRecordType::from_u8(self.record_type)?;

        // Handle special records
        if record_type == QSRecordType::QS_TARGET_INFO {
            // Parse and store target configuration
            if let Some(config) = TargetConfig::from_data(&self.data_buffer) {
                println!("<TARGET> INFO     QP ver={}.{}.{} Build:{}",
                    config.qp_version >> 8,
                    (config.qp_version >> 4) & 0x0F,
                    config.qp_version & 0x0F,
                    config.target_name
                );
                println!("<TARGET> CONFIG   TS:{} SIG:{} OBJ:{} FUN:{} EVT:{} QC:{} PC:{} PB:{} TC:{}",
                    config.qs_time_size,
                    config.signal_size,
                    config.obj_ptr_size,
                    config.fun_ptr_size,
                    config.event_size,
                    config.queue_ctr_size,
                    config.pool_ctr_size,
                    config.pool_blk_size,
                    config.time_evt_ctr_size
                );
                self.target_config = config;
            }
        }

        // Increment timestamp
        self.timestamp += 1;

        // Create record with data
        let record_data = self.data_buffer.clone();
        Some(QSRecord::new(self.timestamp, record_type, record_data))
    }

    /// Validate checksum of current frame
    fn validate_checksum(&self) -> bool {
        // Checksum = ~(sequence + record_type + sum(data))
        let mut sum: u8 = self.sequence;
        sum = sum.wrapping_add(self.record_type);
        for &byte in &self.data_buffer {
            sum = sum.wrapping_add(byte);
        }
        let computed_checksum = !sum;
        computed_checksum == self.checksum
    }

    /// Reset frame parser state for next frame
    fn reset_frame(&mut self) {
        self.state = ParserState::WaitingForData;
        self.sequence = 0;
        self.record_type = 0;
        self.data_buffer.clear();
        self.checksum = 0;
        self.escape_next = false;
    }

    /// Get statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.frames_received, self.checksum_errors, self.sequence_gaps)
    }
}

impl Default for QSParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_packet() {
        let mut parser = QSParser::new();
        
        // Create a simple packet: seq(0) + SM_TRAN(6) + data
        let packet = vec![0, 6, 1, b'T', b'E', b'S', b'T', 0];
        
        let records = parser.parse_packet(&packet);
        assert!(records.is_some());
        
        let records = records.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record_type, QSRecordType::QS_QEP_TRAN);
    }

    #[test]
    fn test_empty_packet() {
        let mut parser = QSParser::new();
        let packet = vec![];
        assert!(parser.parse_packet(&packet).is_none());
    }

    #[test]
    fn test_timestamp_increment() {
        let mut parser = QSParser::new();
        
        let packet1 = vec![0, 6, 0];
        let records1 = parser.parse_packet(&packet1).unwrap();
        assert_eq!(records1[0].timestamp, 1);
        
        let packet2 = vec![1, 6, 0];
        let records2 = parser.parse_packet(&packet2).unwrap();
        assert_eq!(records2[0].timestamp, 2);
    }
}
