//! QS Buffer Management
//!
//! Handles trace data buffering, HDLC framing, and output.

use crate::types::{QSConfig, QSRecordType};

/// HDLC protocol constants
pub mod hdlc {
    /// HDLC flag byte (frame delimiter)
    pub const FLAG: u8 = 0x7E;
    /// HDLC escape byte
    pub const ESC: u8 = 0x7D;
    /// XOR mask for escaped bytes
    pub const ESC_XOR: u8 = 0x20;
}

/// QS buffer for collecting trace data
pub struct QSBuffer<const N: usize> {
    /// Ring buffer storage
    data: [u8; N],
    /// Write index (head)
    head: usize,
    /// Read index (tail)
    tail: usize,
    /// Number of bytes in buffer
    used: usize,
    /// Sequence number for HDLC frames
    sequence: u8,
    /// Current record being built
    current_record: Option<RecordBuilder>,
    /// Global filter (128-bit mask for record types)
    global_filter: u128,
    /// Local filter (128-bit mask for QS-IDs)
    local_filter: u128,
    /// Configuration
    config: QSConfig,
    /// Timestamp counter
    timestamp: u64,
}

/// Record builder for constructing trace records
struct RecordBuilder {
    record_type: QSRecordType,
    timestamp: u64,
    data: heapless::Vec<u8, 256>,
    qs_id: u8,
}

impl<const N: usize> QSBuffer<N> {
    /// Create new QS buffer
    pub const fn new() -> Self {
        Self {
            data: [0; N],
            head: 0,
            tail: 0,
            used: 0,
            sequence: 0,
            current_record: None,
            global_filter: 0, // All OFF by default per spec
            local_filter: u128::MAX, // All ON by default per spec
            config: QSConfig {
                time_size: 4,
                signal_size: 2,
                event_size: 4,
                queue_ctr_size: 2,
                pool_ctr_size: 2,
                pool_blk_size: 2,
                time_evt_ctr_size: 4,
                obj_ptr_size: core::mem::size_of::<usize>() as u8,
                fun_ptr_size: core::mem::size_of::<usize>() as u8,
            },
            timestamp: 0,
        }
    }

    /// Initialize the QS buffer
    pub fn init(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.used = 0;
        self.sequence = 0;
        self.current_record = None;
        self.global_filter = 0;
        self.local_filter = u128::MAX;
        self.timestamp = 0;
    }

    /// Set configuration
    pub fn set_config(&mut self, config: QSConfig) {
        self.config = config;
    }

    /// Get configuration
    pub fn config(&self) -> &QSConfig {
        &self.config
    }

    /// Set global filter for record type
    pub fn set_global_filter(&mut self, record_type: QSRecordType, enable: bool) {
        let bit = record_type as u8;
        if bit < 128 {
            if enable {
                self.global_filter |= 1u128 << bit;
            } else {
                self.global_filter &= !(1u128 << bit);
            }
        }
    }

    /// Set global filter mask directly
    pub fn set_global_filter_mask(&mut self, mask: u128) {
        self.global_filter = mask;
    }

    /// Set local filter for QS-ID
    pub fn set_local_filter(&mut self, qs_id: u8, enable: bool) {
        if qs_id < 128 {
            if enable {
                self.local_filter |= 1u128 << qs_id;
            } else {
                self.local_filter &= !(1u128 << qs_id);
            }
        }
    }

    /// Set local filter mask directly
    pub fn set_local_filter_mask(&mut self, mask: u128) {
        self.local_filter = mask;
    }

    /// Check if record passes filters
    fn passes_filters(&self, record_type: QSRecordType, qs_id: u8) -> bool {
        // Non-maskable records always pass
        if record_type.is_non_maskable() {
            return true;
        }

        // Check global filter
        let rec_bit = record_type as u8;
        let global_pass = rec_bit >= 128 || (self.global_filter & (1u128 << rec_bit)) != 0;

        // Check local filter (QS-ID 0 always passes)
        let local_pass = qs_id == 0 || qs_id >= 128 || (self.local_filter & (1u128 << qs_id)) != 0;

        global_pass && local_pass
    }

    /// Begin a trace record
    pub fn begin(&mut self, record_type: QSRecordType, qs_id: u8) -> bool {
        // Check filters
        if !self.passes_filters(record_type, qs_id) {
            return false;
        }

        // End any current record
        if self.current_record.is_some() {
            self.end();
        }

        // Increment timestamp
        self.timestamp = self.timestamp.wrapping_add(1);

        // Start new record
        self.current_record = Some(RecordBuilder {
            record_type,
            timestamp: self.timestamp,
            data: heapless::Vec::new(),
            qs_id,
        });

        true
    }

    /// End current trace record and commit to buffer
    pub fn end(&mut self) {
        if let Some(record) = self.current_record.take() {
            self.commit_record(record);
        }
    }

    /// Commit record to buffer with HDLC framing
    fn commit_record(&mut self, record: RecordBuilder) {
        // Build HDLC frame
        let mut frame = heapless::Vec::<u8, 512>::new();

        // Calculate checksum: ~(sequence + record_type + sum(timestamp + data))
        let mut checksum: u8 = self.sequence;
        checksum = checksum.wrapping_add(record.record_type as u8);

        // Add timestamp bytes to checksum
        let ts_bytes = self.timestamp_bytes(record.timestamp);
        for &byte in &ts_bytes {
            checksum = checksum.wrapping_add(byte);
        }

        // Add data bytes to checksum
        for &byte in &record.data {
            checksum = checksum.wrapping_add(byte);
        }
        checksum = !checksum;

        // Helper to add byte with stuffing
        let add_byte = |frame: &mut heapless::Vec<u8, 512>, byte: u8| {
            if byte == hdlc::FLAG || byte == hdlc::ESC {
                let _ = frame.push(hdlc::ESC);
                let _ = frame.push(byte ^ hdlc::ESC_XOR);
            } else {
                let _ = frame.push(byte);
            }
        };

        // Build frame: sequence + record_type + timestamp + data + checksum + flag
        add_byte(&mut frame, self.sequence);
        add_byte(&mut frame, record.record_type as u8);

        // Add timestamp
        for &byte in &ts_bytes {
            add_byte(&mut frame, byte);
        }

        // Add data
        for &byte in &record.data {
            add_byte(&mut frame, byte);
        }

        // Add checksum
        add_byte(&mut frame, checksum);

        // Add flag (never stuffed)
        let _ = frame.push(hdlc::FLAG);

        // Write frame to ring buffer
        for byte in &frame {
            self.write_byte(*byte);
        }

        // Increment sequence number
        self.sequence = self.sequence.wrapping_add(1);
    }

    /// Write byte to ring buffer
    fn write_byte(&mut self, byte: u8) {
        if self.used < N {
            self.data[self.head] = byte;
            self.head = (self.head + 1) % N;
            self.used += 1;
        }
        // Note: If buffer is full, byte is dropped (overflow)
    }

    /// Read available bytes from buffer
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut n = 0;
        while n < buf.len() && self.used > 0 {
            buf[n] = self.data[self.tail];
            self.tail = (self.tail + 1) % N;
            self.used -= 1;
            n += 1;
        }
        n
    }

    /// Get number of bytes available to read
    pub fn available(&self) -> usize {
        self.used
    }

    /// Get timestamp as bytes
    fn timestamp_bytes(&self, timestamp: u64) -> heapless::Vec<u8, 8> {
        let mut bytes = heapless::Vec::new();
        match self.config.time_size {
            1 => {
                let _ = bytes.push(timestamp as u8);
            }
            2 => {
                for byte in (timestamp as u16).to_le_bytes() {
                    let _ = bytes.push(byte);
                }
            }
            4 => {
                for byte in (timestamp as u32).to_le_bytes() {
                    let _ = bytes.push(byte);
                }
            }
            _ => {
                // Default to 4 bytes
                for byte in (timestamp as u32).to_le_bytes() {
                    let _ = bytes.push(byte);
                }
            }
        }
        bytes
    }

    // Data output methods for building records

    /// Add u8 to current record
    pub fn u8(&mut self, value: u8) {
        if let Some(ref mut record) = self.current_record {
            let _ = record.data.push(value);
        }
    }

    /// Add i8 to current record
    pub fn i8(&mut self, value: i8) {
        self.u8(value as u8);
    }

    /// Add u16 to current record
    pub fn u16(&mut self, value: u16) {
        if let Some(ref mut record) = self.current_record {
            for byte in value.to_le_bytes() {
                let _ = record.data.push(byte);
            }
        }
    }

    /// Add i16 to current record
    pub fn i16(&mut self, value: i16) {
        self.u16(value as u16);
    }

    /// Add u32 to current record
    pub fn u32(&mut self, value: u32) {
        if let Some(ref mut record) = self.current_record {
            for byte in value.to_le_bytes() {
                let _ = record.data.push(byte);
            }
        }
    }

    /// Add i32 to current record
    pub fn i32(&mut self, value: i32) {
        self.u32(value as u32);
    }

    /// Add u64 to current record
    pub fn u64(&mut self, value: u64) {
        if let Some(ref mut record) = self.current_record {
            for byte in value.to_le_bytes() {
                let _ = record.data.push(byte);
            }
        }
    }

    /// Add i64 to current record
    pub fn i64(&mut self, value: i64) {
        self.u64(value as u64);
    }

    /// Add f32 to current record
    pub fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    /// Add f64 to current record
    pub fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }

    /// Add string to current record (zero-terminated)
    pub fn str(&mut self, value: &str) {
        if let Some(ref mut record) = self.current_record {
            for byte in value.as_bytes() {
                let _ = record.data.push(*byte);
            }
            let _ = record.data.push(0); // null terminator
        }
    }

    /// Add memory block to current record
    pub fn mem(&mut self, data: &[u8], len: u8) {
        if let Some(ref mut record) = self.current_record {
            let _ = record.data.push(len);
            for &byte in data.iter().take(len as usize) {
                let _ = record.data.push(byte);
            }
        }
    }

    /// Add object pointer (configured size)
    pub fn obj_ptr(&mut self, ptr: usize) {
        match self.config.obj_ptr_size {
            2 => self.u16(ptr as u16),
            4 => self.u32(ptr as u32),
            8 => self.u64(ptr as u64),
            _ => self.u32(ptr as u32),
        }
    }

    /// Add function pointer (configured size)
    pub fn fun_ptr(&mut self, ptr: usize) {
        match self.config.fun_ptr_size {
            2 => self.u16(ptr as u16),
            4 => self.u32(ptr as u32),
            8 => self.u64(ptr as u64),
            _ => self.u32(ptr as u32),
        }
    }

    /// Add signal (configured size)
    pub fn signal(&mut self, sig: u32) {
        match self.config.signal_size {
            1 => self.u8(sig as u8),
            2 => self.u16(sig as u16),
            4 => self.u32(sig),
            _ => self.u16(sig as u16),
        }
    }

    /// Add event pointer (configured size)
    pub fn evt_ptr(&mut self, ptr: usize) {
        match self.config.event_size {
            2 => self.u16(ptr as u16),
            4 => self.u32(ptr as u32),
            8 => self.u64(ptr as u64),
            _ => self.u32(ptr as u32),
        }
    }

    /// Add queue counter (configured size)
    pub fn queue_ctr(&mut self, ctr: u32) {
        match self.config.queue_ctr_size {
            1 => self.u8(ctr as u8),
            2 => self.u16(ctr as u16),
            4 => self.u32(ctr),
            _ => self.u16(ctr as u16),
        }
    }

    /// Add pool counter (configured size)
    pub fn pool_ctr(&mut self, ctr: u32) {
        match self.config.pool_ctr_size {
            1 => self.u8(ctr as u8),
            2 => self.u16(ctr as u16),
            4 => self.u32(ctr),
            _ => self.u16(ctr as u16),
        }
    }

    /// Add pool block size (configured size)
    pub fn pool_blk(&mut self, size: u32) {
        match self.config.pool_blk_size {
            1 => self.u8(size as u8),
            2 => self.u16(size as u16),
            4 => self.u32(size),
            _ => self.u16(size as u16),
        }
    }

    /// Add time event counter (configured size)
    pub fn te_ctr(&mut self, ctr: u32) {
        match self.config.time_evt_ctr_size {
            1 => self.u8(ctr as u8),
            2 => self.u16(ctr as u16),
            4 => self.u32(ctr),
            _ => self.u32(ctr),
        }
    }

    /// Generate QS_TARGET_INFO record (non-maskable)
    ///
    /// Contains: QP version, endianness, 9 config sizes, target name
    pub fn target_info_record(&mut self, qp_version: &str, target_name: &str, endianness: u8) {
        let mut data: heapless::Vec<u8, 256> = heapless::Vec::new();
        
        // QP version string (null-terminated)
        for &b in qp_version.as_bytes() {
            data.push(b).ok();
        }
        data.push(0).ok();
        
        // Endianness (0=little, 1=big)
        data.push(endianness).ok();
        
        // Configuration sizes (9 bytes)
        data.push(self.config.time_size).ok();
        data.push(self.config.signal_size).ok();
        data.push(self.config.event_size).ok();
        data.push(self.config.queue_ctr_size).ok();
        data.push(self.config.pool_ctr_size).ok();
        data.push(self.config.pool_blk_size).ok();
        data.push(self.config.time_evt_ctr_size).ok();
        data.push(self.config.obj_ptr_size).ok();
        data.push(self.config.fun_ptr_size).ok();
        
        // Target name string (null-terminated)
        for &b in target_name.as_bytes() {
            data.push(b).ok();
        }
        data.push(0).ok();
        
        let builder = RecordBuilder {
            record_type: QSRecordType::QS_TARGET_INFO,
            timestamp: 0, // No timestamp for target info
            data,
            qs_id: 0,
        };
        
        self.commit_record(builder);
    }
}

impl<const N: usize> Default for QSBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}
