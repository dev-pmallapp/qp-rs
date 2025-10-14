//! Record Formatter
//!
//! Formats QS trace records for display

use crate::protocol::{QSRecord, RecordGroup};
use colored::Colorize;
use std::collections::HashSet;

pub struct RecordFormatter {
    show_timestamps: bool,
    json_format: bool,
    filters: Option<HashSet<RecordGroup>>,
}

impl RecordFormatter {
    pub fn new(show_timestamps: bool, json_format: bool) -> Self {
        Self {
            show_timestamps,
            json_format,
            filters: None,
        }
    }

    pub fn set_filters(&mut self, filter_names: &[String]) {
        let mut filters = HashSet::new();
        for name in filter_names {
            match name.to_lowercase().as_str() {
                "sm" | "statemachine" => filters.insert(RecordGroup::StateMachine),
                "ao" | "activeobject" => filters.insert(RecordGroup::ActiveObject),
                "eq" | "eventqueue" => filters.insert(RecordGroup::EventQueue),
                "mp" | "memorypool" => filters.insert(RecordGroup::MemoryPool),
                "te" | "timeevent" => filters.insert(RecordGroup::TimeEvent),
                "sched" | "scheduler" => filters.insert(RecordGroup::Scheduler),
                "sem" | "semaphore" => filters.insert(RecordGroup::Semaphore),
                "mtx" | "mutex" => filters.insert(RecordGroup::Mutex),
                "user" => filters.insert(RecordGroup::User),
                "info" => filters.insert(RecordGroup::Info),
                "dict" | "dictionary" => filters.insert(RecordGroup::Dictionary),
                "test" => filters.insert(RecordGroup::Test),
                "err" | "error" => filters.insert(RecordGroup::Error),
                "qf" | "framework" => filters.insert(RecordGroup::Framework),
                _ => false,
            };
        }
        self.filters = Some(filters);
    }

    pub fn format_record(&self, record: &QSRecord) {
        let group = record.record_type.group();

        // Apply filters if set
        if let Some(ref filters) = self.filters {
            if !filters.contains(&group) {
                return; // Skip this record
            }
        }

        if self.json_format {
            self.format_json(record);
        } else {
            self.format_text(record);
        }
    }

    fn format_text(&self, record: &QSRecord) {
        let group = record.record_type.group();
        let record_name = record.record_type.name();

        // Color based on group
        let colored_name = match group {
            RecordGroup::StateMachine => record_name.bright_blue(),
            RecordGroup::ActiveObject => record_name.bright_green(),
            RecordGroup::EventQueue => record_name.bright_cyan(),
            RecordGroup::MemoryPool => record_name.bright_magenta(),
            RecordGroup::TimeEvent => record_name.bright_yellow(),
            RecordGroup::Scheduler => record_name.bright_white(),
            RecordGroup::Semaphore => record_name.cyan(),
            RecordGroup::Mutex => record_name.magenta(),
            RecordGroup::Framework => record_name.green(),
            RecordGroup::Dictionary => record_name.yellow(),
            RecordGroup::Test => record_name.blue(),
            RecordGroup::Error => record_name.bright_red().bold(),
            RecordGroup::User => record_name.white(),
            RecordGroup::Info => record_name.bright_white(),
        };

        // Format timestamp if enabled
        let timestamp_str = if self.show_timestamps {
            format!("[{:08}] ", record.timestamp).dimmed().to_string()
        } else {
            String::new()
        };

        // Format data
        let data_str = self.format_data(record);

        println!("{}{:16} {}", timestamp_str, colored_name, data_str);
    }

    fn format_json(&self, record: &QSRecord) {
        let data_hex: String = record
            .data
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("");

        let json = serde_json::json!({
            "timestamp": record.timestamp,
            "type": record.record_type.name(),
            "group": format!("{:?}", record.record_type.group()),
            "data": data_hex,
        });

        println!("{}", serde_json::to_string(&json).unwrap());
    }

    fn format_data(&self, record: &QSRecord) -> String {
        if record.data.is_empty() {
            return String::new();
        }

        // Try to interpret data based on record type
        match record.record_type.group() {
            RecordGroup::StateMachine => {
                // SM records often have: obj_id, state_name
                if record.data.len() >= 4 {
                    let obj_id = u32::from_le_bytes([
                        record.data[0],
                        record.data[1],
                        record.data[2],
                        record.data[3],
                    ]);
                    
                    let mut result = format!("obj={:08x}", obj_id);
                    
                    // Look for string data after first 4 bytes
                    if record.data.len() > 4 {
                        if let Some(str_end) = record.data[4..].iter().position(|&b| b == 0) {
                            if let Ok(s) = std::str::from_utf8(&record.data[4..4 + str_end]) {
                                result.push_str(&format!(" {}", s.bright_cyan()));
                            }
                            
                            // Check for additional u32 values
                            let remaining_start = 4 + str_end + 1;
                            if remaining_start + 4 <= record.data.len() {
                                let value = u32::from_le_bytes([
                                    record.data[remaining_start],
                                    record.data[remaining_start + 1],
                                    record.data[remaining_start + 2],
                                    record.data[remaining_start + 3],
                                ]);
                                result.push_str(&format!(" cycles={}", value.to_string().bright_yellow()));
                            }
                        }
                    }
                    
                    result
                } else {
                    // Fallback to hex dump
                    self.format_hex(&record.data)
                }
            }
            RecordGroup::Dictionary => {
                // Dictionary records contain key=value pairs
                if let Ok(s) = std::str::from_utf8(&record.data) {
                    s.trim_end_matches('\0').to_string()
                } else {
                    self.format_hex(&record.data)
                }
            }
            RecordGroup::Info | RecordGroup::Test => {
                // Info/test records often contain strings
                if let Ok(s) = std::str::from_utf8(&record.data) {
                    s.trim_end_matches('\0').to_string()
                } else {
                    self.format_hex(&record.data)
                }
            }
            RecordGroup::User => {
                // User records - try string first, then hex
                if let Ok(s) = std::str::from_utf8(&record.data) {
                    let clean = s.trim_end_matches('\0');
                    if clean.chars().all(|c| c.is_ascii() && !c.is_control() || c == '\n' || c == '\r') {
                        return clean.bright_white().to_string();
                    }
                }
                self.format_hex(&record.data)
            }
            _ => {
                // Default: hex dump for other records
                self.format_hex(&record.data)
            }
        }
    }

    fn format_hex(&self, data: &[u8]) -> String {
        data.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
            .dimmed()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::QSRecordType;

    #[test]
    fn test_format_empty_data() {
        let formatter = RecordFormatter::new(false, false);
        let record = QSRecord::new(1, QSRecordType::QS_EMPTY, vec![]);
        formatter.format_record(&record);
        // Should not panic
    }

    #[test]
    fn test_filter_setup() {
        let mut formatter = RecordFormatter::new(false, false);
        formatter.set_filters(&["sm".to_string(), "ao".to_string()]);
        
        assert!(formatter.filters.is_some());
        let filters = formatter.filters.as_ref().unwrap();
        assert!(filters.contains(&RecordGroup::StateMachine));
        assert!(filters.contains(&RecordGroup::ActiveObject));
        assert!(!filters.contains(&RecordGroup::EventQueue));
    }
}
