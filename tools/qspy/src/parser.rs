//! QS Packet Parser
//!
//! Parses incoming UDP packets containing QS trace records

use crate::protocol::{QSRecord, QSRecordType};

pub struct QSParser {
    timestamp: u64,
}

impl QSParser {
    pub fn new() -> Self {
        Self { timestamp: 0 }
    }

    /// Parse a UDP packet containing one or more QS records
    pub fn parse_packet(&mut self, data: &[u8]) -> Option<Vec<QSRecord>> {
        if data.is_empty() {
            return None;
        }

        let mut records = Vec::new();
        let mut offset = 0;

        // First byte is sequence number
        let _seq_num = data[0];
        offset += 1;

        // Parse records until end of packet
        while offset < data.len() {
            if let Some((record, bytes_read)) = self.parse_record(&data[offset..]) {
                records.push(record);
                offset += bytes_read;
            } else {
                // Failed to parse record, skip rest of packet
                break;
            }
        }

        if records.is_empty() {
            None
        } else {
            Some(records)
        }
    }

    /// Parse a single QS record
    fn parse_record(&mut self, data: &[u8]) -> Option<(QSRecord, usize)> {
        if data.is_empty() {
            return None;
        }

        let record_type_byte = data[0];
        let record_type = QSRecordType::from_u8(record_type_byte)?;

        let mut offset = 1;
        
        // Increment timestamp for each record
        self.timestamp += 1;

        // Parse record data based on type
        let record_data = match record_type {
            QSRecordType::QS_QEP_TRAN => {
                // SM_TRAN has: philo_id, string, time
                if offset + 1 < data.len() {
                    let philo_id = data[offset];
                    offset += 1;

                    // Find null-terminated string
                    let str_start = offset;
                    while offset < data.len() && data[offset] != 0 {
                        offset += 1;
                    }
                    let str_bytes = &data[str_start..offset];
                    offset += 1; // skip null terminator

                    // Read u32 time value if present
                    let mut full_data = vec![philo_id];
                    full_data.extend_from_slice(str_bytes);
                    full_data.push(0); // null terminator

                    if offset + 4 <= data.len() {
                        let time_bytes = &data[offset..offset + 4];
                        full_data.extend_from_slice(time_bytes);
                        offset += 4;
                    }

                    full_data
                } else {
                    vec![]
                }
            }
            QSRecordType::QS_TARGET_INFO => {
                // TARGET_INFO contains version and other info
                // Read until null terminator or end
                let start = offset;
                while offset < data.len() && data[offset] != 0 {
                    offset += 1;
                }
                let info_data = data[start..offset].to_vec();
                if offset < data.len() {
                    offset += 1; // skip null terminator
                }
                info_data
            }
            QSRecordType::QS_USER => {
                // User records - variable length until end of record
                // Typically ends with null terminator or has fixed length
                let start = offset;
                while offset < data.len() && data[offset] != 0 {
                    offset += 1;
                }
                let user_data = data[start..offset].to_vec();
                if offset < data.len() {
                    offset += 1; // skip null terminator
                }
                user_data
            }
            _ => {
                // Generic parsing - read fixed size or until delimiter
                // For now, read up to 32 bytes or until null terminator
                let start = offset;
                let max_len = (data.len() - offset).min(32);
                let end = start + max_len;
                
                // Look for null terminator within range
                while offset < end {
                    if data[offset] == 0 {
                        offset += 1;
                        break;
                    }
                    offset += 1;
                }
                
                data[start..offset].to_vec()
            }
        };

        Some((QSRecord::new(self.timestamp, record_type, record_data), offset))
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
