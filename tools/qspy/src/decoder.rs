//! QS Record Decoder
//!
//! Decodes QS trace records into human-readable format based on QP/C specification

use crate::protocol::{QSRecord, QSRecordType, TargetConfig};
use colored::Colorize;

/// Dictionary for resolving object/function/signal addresses to names
pub struct Dictionary {
    pub objects: std::collections::HashMap<u64, String>,
    pub functions: std::collections::HashMap<u64, String>,
    pub signals: std::collections::HashMap<u16, String>,
    pub user_records: std::collections::HashMap<u8, String>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self {
            objects: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            signals: std::collections::HashMap::new(),
            user_records: std::collections::HashMap::new(),
        }
    }
}

/// Record decoder with target configuration and dictionary
pub struct RecordDecoder {
    config: TargetConfig,
    dict: Dictionary,
}

impl RecordDecoder {
    pub fn new() -> Self {
        Self {
            config: TargetConfig::default(),
            dict: Dictionary::new(),
        }
    }

    pub fn set_config(&mut self, config: TargetConfig) {
        self.config = config;
    }

    pub fn dictionary_mut(&mut self) -> &mut Dictionary {
        &mut self.dict
    }

    /// Decode a record into human-readable string
    pub fn decode(&self, record: &QSRecord) -> String {
        use QSRecordType::*;

        match record.record_type {
            // State Machine records
            QS_QEP_STATE_ENTRY => self.decode_state_entry(record),
            QS_QEP_STATE_EXIT => self.decode_state_exit(record),
            QS_QEP_STATE_INIT => self.decode_state_init(record),
            QS_QEP_INIT_TRAN => self.decode_init_tran(record),
            QS_QEP_INTERN_TRAN => self.decode_intern_tran(record),
            QS_QEP_TRAN => self.decode_tran(record),
            QS_QEP_IGNORED => self.decode_ignored(record),
            QS_QEP_DISPATCH => self.decode_dispatch(record),

            // Active Object records
            QS_QF_ACTIVE_SUBSCRIBE => self.decode_ao_subscribe(record),
            QS_QF_ACTIVE_POST => self.decode_ao_post(record),
            QS_QF_ACTIVE_POST_LIFO => self.decode_ao_post_lifo(record),
            QS_QF_ACTIVE_GET => self.decode_ao_get(record),
            QS_QF_ACTIVE_GET_LAST => self.decode_ao_get_last(record),

            // Memory Pool records
            QS_QF_MPOOL_GET => self.decode_mp_get(record),
            QS_QF_MPOOL_PUT => self.decode_mp_put(record),

            // Framework records
            QS_QF_NEW => self.decode_qf_new(record),
            QS_QF_GC => self.decode_qf_gc(record),
            QS_QF_PUBLISH => self.decode_qf_publish(record),

            // Time Event records
            QS_QF_TIMEEVT_ARM => self.decode_te_arm(record),
            QS_QF_TIMEEVT_AUTO_DISARM => self.decode_te_auto_disarm(record),
            QS_QF_TIMEEVT_DISARM_ATTEMPT => self.decode_te_disarm_attempt(record),
            QS_QF_TIMEEVT_DISARM => self.decode_te_disarm(record),
            QS_QF_TIMEEVT_POST => self.decode_te_post(record),

            // User records
            _ if record.record_type as u8 >= 100 => self.decode_user_record(record),

            _ => format!("data={}", self.format_hex(&record.data)),
        }
    }

    // State Machine record decoders

    fn decode_state_entry(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(state)) = (reader.read_obj(), reader.read_fun()) {
            format!("obj={}  cycles={}",
                self.resolve_obj(obj).bright_cyan(),
                state.to_string().bright_yellow())
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_state_exit(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(state)) = (reader.read_obj(), reader.read_fun()) {
            format!("obj={}  cycles={}",
                self.resolve_obj(obj).bright_cyan(),
                state.to_string().bright_yellow())
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_state_init(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(src), Some(trg)) = (reader.read_obj(), reader.read_fun(), reader.read_fun()) {
            format!("obj={},State={}->{}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_fun(src),
                self.resolve_fun(trg))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_init_tran(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(state)) = (reader.read_obj(), reader.read_fun()) {
            format!("obj={},State={}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_fun(state))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_intern_tran(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(sig), Some(state)) = (reader.read_obj(), reader.read_sig(), reader.read_fun()) {
            format!("obj={},Sig={},State={}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_sig(sig),
                self.resolve_fun(state))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_tran(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(sig), Some(src), Some(trg)) = 
            (reader.read_obj(), reader.read_sig(), reader.read_fun(), reader.read_fun()) {
            format!("obj={},Sig={},State={}->{}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_sig(sig),
                self.resolve_fun(src),
                self.resolve_fun(trg))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_ignored(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(sig), Some(state)) = (reader.read_obj(), reader.read_sig(), reader.read_fun()) {
            format!("obj={},Sig={},State={}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_sig(sig),
                self.resolve_fun(state))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_dispatch(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(sig), Some(state)) = (reader.read_obj(), reader.read_sig(), reader.read_fun()) {
            format!("obj={},Sig={},State={}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_sig(sig),
                self.resolve_fun(state))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    // Active Object record decoders

    fn decode_ao_subscribe(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(obj), Some(sig)) = (reader.read_obj(), reader.read_sig()) {
            format!("Obj={},Sig={}",
                self.resolve_obj(obj).bright_cyan(),
                self.resolve_sig(sig))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_ao_post(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(sender), Some(obj)) = (reader.read_obj(), reader.read_obj()) {
            let sig = reader.read_sig();
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            let free = reader.read_queue_ctr();
            let min = reader.read_queue_ctr();
            
            format!("Sdr={},Obj={},Evt<Sig={},Pool={},Ref={}>,Que<Free={},Min={}>",
                self.resolve_obj(sender),
                self.resolve_obj(obj).bright_cyan(),
                sig.map_or("?".to_string(), |s| self.resolve_sig(s)),
                pool.unwrap_or(0),
                refctr.unwrap_or(0),
                free.unwrap_or(0),
                min.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_ao_post_lifo(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(obj) = reader.read_obj() {
            let sig = reader.read_sig();
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            let free = reader.read_queue_ctr();
            let min = reader.read_queue_ctr();
            
            format!("Obj={},Evt<Sig={},Pool={},Ref={}>,Que<Free={},Min={}>",
                self.resolve_obj(obj).bright_cyan(),
                sig.map_or("?".to_string(), |s| self.resolve_sig(s)),
                pool.unwrap_or(0),
                refctr.unwrap_or(0),
                free.unwrap_or(0),
                min.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_ao_get(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(obj) = reader.read_obj() {
            let sig = reader.read_sig();
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            
            format!("Obj={},Evt<Sig={},Pool={},Ref={}>",
                self.resolve_obj(obj).bright_cyan(),
                sig.map_or("?".to_string(), |s| self.resolve_sig(s)),
                pool.unwrap_or(0),
                refctr.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_ao_get_last(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(obj) = reader.read_obj() {
            let sig = reader.read_sig();
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            
            format!("Obj={},Evt<Sig={},Pool={},Ref={}>",
                self.resolve_obj(obj).bright_cyan(),
                sig.map_or("?".to_string(), |s| self.resolve_sig(s)),
                pool.unwrap_or(0),
                refctr.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    // Memory Pool record decoders

    fn decode_mp_get(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(obj) = reader.read_obj() {
            let free = reader.read_pool_ctr();
            let min = reader.read_pool_ctr();
            
            format!("Obj={},Free={},Min={}",
                self.resolve_obj(obj).bright_cyan(),
                free.unwrap_or(0),
                min.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_mp_put(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(obj) = reader.read_obj() {
            let free = reader.read_pool_ctr();
            
            format!("Obj={},Free={}",
                self.resolve_obj(obj).bright_cyan(),
                free.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    // Framework record decoders

    fn decode_qf_new(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(sig), Some(size)) = (reader.read_sig(), reader.read_event_size()) {
            format!("Sig={},Size={}",
                self.resolve_sig(sig),
                size)
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_qf_gc(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(sig) = reader.read_sig() {
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            
            format!("Evt<Sig={},Pool={},Ref={}>",
                self.resolve_sig(sig),
                pool.unwrap_or(0),
                refctr.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_qf_publish(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let Some(sig) = reader.read_sig() {
            let pool = reader.read_u8();
            let refctr = reader.read_u8();
            
            format!("Sig={},Pool={},Ref={}",
                self.resolve_sig(sig),
                pool.unwrap_or(0),
                refctr.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    // Time Event record decoders

    fn decode_te_arm(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(te_obj), Some(ao_obj)) = (reader.read_obj(), reader.read_obj()) {
            let tim = reader.read_te_ctr();
            let interval = reader.read_te_ctr();
            
            format!("Obj={},AO={},Tim={},Int={}",
                self.resolve_obj(te_obj).bright_cyan(),
                self.resolve_obj(ao_obj),
                tim.unwrap_or(0),
                interval.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_te_auto_disarm(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(te_obj), Some(ao_obj)) = (reader.read_obj(), reader.read_obj()) {
            format!("Obj={},AO={}",
                self.resolve_obj(te_obj).bright_cyan(),
                self.resolve_obj(ao_obj))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_te_disarm_attempt(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(te_obj), Some(ao_obj)) = (reader.read_obj(), reader.read_obj()) {
            format!("Obj={},AO={}",
                self.resolve_obj(te_obj).bright_cyan(),
                self.resolve_obj(ao_obj))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_te_disarm(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(te_obj), Some(ao_obj)) = (reader.read_obj(), reader.read_obj()) {
            let was_armed = reader.read_u8();
            
            format!("Obj={},AO={},Armed={}",
                self.resolve_obj(te_obj).bright_cyan(),
                self.resolve_obj(ao_obj),
                was_armed.unwrap_or(0))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    fn decode_te_post(&self, record: &QSRecord) -> String {
        let mut reader = DataReader::new(&record.data, &self.config);
        
        if let (Some(te_obj), Some(sig), Some(ao_obj)) = (reader.read_obj(), reader.read_sig(), reader.read_obj()) {
            format!("Obj={},Sig={},AO={}",
                self.resolve_obj(te_obj).bright_cyan(),
                self.resolve_sig(sig),
                self.resolve_obj(ao_obj))
        } else {
            format!("data={}", self.format_hex(&record.data))
        }
    }

    // User record decoder

    fn decode_user_record(&self, record: &QSRecord) -> String {
        let rec_id = record.record_type as u8;
        let name = self.dict.user_records.get(&rec_id)
            .map(|s| s.as_str())
            .unwrap_or("USER");
        
        // Try to decode user record data
        if record.data.is_empty() {
            return String::new();
        }
        
        // Check if it contains a string
        if let Some(null_pos) = record.data.iter().position(|&b| b == 0) {
            if let Ok(s) = std::str::from_utf8(&record.data[..null_pos]) {
                return format!("{} {}", name, s.bright_white());
            }
        }
        
        format!("{} data={}", name, self.format_hex(&record.data))
    }

    // Helper functions

    fn resolve_obj(&self, addr: u64) -> String {
        self.dict.objects.get(&addr)
            .map(|s| s.clone())
            .unwrap_or_else(|| format!("{:08X}", addr))
    }

    fn resolve_fun(&self, addr: u64) -> String {
        self.dict.functions.get(&addr)
            .map(|s| s.clone())
            .unwrap_or_else(|| format!("{:08X}", addr))
    }

    fn resolve_sig(&self, sig: u16) -> String {
        self.dict.signals.get(&sig)
            .map(|s| s.clone())
            .unwrap_or_else(|| format!("{}", sig))
    }

    fn format_hex(&self, data: &[u8]) -> String {
        data.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Helper to read data from record buffer according to target configuration
struct DataReader<'a> {
    data: &'a [u8],
    pos: usize,
    config: &'a TargetConfig,
}

impl<'a> DataReader<'a> {
    fn new(data: &'a [u8], config: &'a TargetConfig) -> Self {
        Self { data, pos: 0, config }
    }

    fn read_u8(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let val = self.data[self.pos];
            self.pos += 1;
            Some(val)
        } else {
            None
        }
    }

    fn read_u16(&mut self) -> Option<u16> {
        if self.pos + 2 <= self.data.len() {
            let val = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            Some(val)
        } else {
            None
        }
    }

    fn read_u32(&mut self) -> Option<u32> {
        if self.pos + 4 <= self.data.len() {
            let val = u32::from_le_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ]);
            self.pos += 4;
            Some(val)
        } else {
            None
        }
    }

    fn read_u64(&mut self) -> Option<u64> {
        if self.pos + 8 <= self.data.len() {
            let val = u64::from_le_bytes([
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
                self.data[self.pos + 4],
                self.data[self.pos + 5],
                self.data[self.pos + 6],
                self.data[self.pos + 7],
            ]);
            self.pos += 8;
            Some(val)
        } else {
            None
        }
    }

    fn read_obj(&mut self) -> Option<u64> {
        match self.config.obj_ptr_size {
            1 => self.read_u8().map(|v| v as u64),
            2 => self.read_u16().map(|v| v as u64),
            4 => self.read_u32().map(|v| v as u64),
            8 => self.read_u64(),
            _ => self.read_u32().map(|v| v as u64),
        }
    }

    fn read_fun(&mut self) -> Option<u64> {
        match self.config.fun_ptr_size {
            1 => self.read_u8().map(|v| v as u64),
            2 => self.read_u16().map(|v| v as u64),
            4 => self.read_u32().map(|v| v as u64),
            8 => self.read_u64(),
            _ => self.read_u32().map(|v| v as u64),
        }
    }

    fn read_sig(&mut self) -> Option<u16> {
        match self.config.signal_size {
            1 => self.read_u8().map(|v| v as u16),
            2 => self.read_u16(),
            4 => self.read_u32().map(|v| v as u16),
            _ => self.read_u16(),
        }
    }

    fn read_event_size(&mut self) -> Option<u16> {
        match self.config.event_size {
            1 => self.read_u8().map(|v| v as u16),
            2 => self.read_u16(),
            4 => self.read_u32().map(|v| v as u16),
            _ => self.read_u16(),
        }
    }

    fn read_queue_ctr(&mut self) -> Option<u16> {
        match self.config.queue_ctr_size {
            1 => self.read_u8().map(|v| v as u16),
            2 => self.read_u16(),
            4 => self.read_u32().map(|v| v as u16),
            _ => self.read_u8().map(|v| v as u16),
        }
    }

    fn read_pool_ctr(&mut self) -> Option<u16> {
        match self.config.pool_ctr_size {
            1 => self.read_u8().map(|v| v as u16),
            2 => self.read_u16(),
            4 => self.read_u32().map(|v| v as u16),
            _ => self.read_u16(),
        }
    }

    fn read_te_ctr(&mut self) -> Option<u32> {
        match self.config.time_evt_ctr_size {
            1 => self.read_u8().map(|v| v as u32),
            2 => self.read_u16().map(|v| v as u32),
            4 => self.read_u32(),
            _ => self.read_u32(),
        }
    }
}
