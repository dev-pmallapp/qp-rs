//! Helpers for emitting predefined QS records such as dictionaries and target
//! information.
//!
//! The official QP implementation sends a set of well-known records when the
//! target establishes a connection with QSPY. The routines below reproduce the
//! payload layout of those records so that the Rust tracer can interoperate
//! with the reference tooling.

/// Record identifier for `QS_SIG_DICT`.
pub const SIG_DICT: u8 = 60;
/// Record identifier for `QS_OBJ_DICT`.
pub const OBJ_DICT: u8 = 61;
/// Record identifier for `QS_FUN_DICT`.
pub const FUN_DICT: u8 = 62;
/// Record identifier for `QS_USR_DICT`.
pub const USR_DICT: u8 = 63;
/// Record identifier for `QS_TARGET_INFO`.
pub const TARGET_INFO: u8 = 64;

/// Helper describing the payload of the `QS_TARGET_INFO` record.
#[derive(Debug, Clone)]
pub struct TargetInfo {
    pub is_reset: u8,
    pub version: u16,
    pub signal_size: u8,
    pub event_size: u8,
    pub equeue_ctr_size: u8,
    pub time_evt_ctr_size: u8,
    pub mpool_size_size: u8,
    pub mpool_ctr_size: u8,
    pub obj_ptr_size: u8,
    pub fun_ptr_size: u8,
    pub time_size: u8,
    pub max_active: u8,
    pub max_event_pools: u8,
    pub max_tick_rate: u8,
    pub build_time: (u8, u8, u8), // (hour, minute, second)
    pub build_date: (u8, u8, u8), // (day, month, year % 100)
}

impl Default for TargetInfo {
    fn default() -> Self {
        Self {
            is_reset: 0xFF,
            version: 740,
            signal_size: 2,
            event_size: 2,
            equeue_ctr_size: 2,
            time_evt_ctr_size: 2,
            mpool_size_size: 2,
            mpool_ctr_size: 2,
            obj_ptr_size: 8,
            fun_ptr_size: 8,
            time_size: 4,
            max_active: 16,
            max_event_pools: 3,
            max_tick_rate: 4,
            build_time: (11, 13, 21),
            build_date: (18, 10, 25),
        }
    }
}

/// Produces the payload bytes for a `QS_TARGET_INFO` record.
pub fn target_info_payload(info: &TargetInfo) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16);
    bytes.push(info.is_reset);
    bytes.extend_from_slice(&info.version.to_le_bytes());
    bytes.push(info.signal_size | (info.event_size << 4));
    bytes.push(info.equeue_ctr_size | (info.time_evt_ctr_size << 4));
    bytes.push(info.mpool_size_size | (info.mpool_ctr_size << 4));
    bytes.push(info.obj_ptr_size | (info.fun_ptr_size << 4));
    bytes.push(info.time_size);
    bytes.push(info.max_active);
    bytes.push(info.max_event_pools | (info.max_tick_rate << 4));

    let (hour, minute, second) = info.build_time;
    bytes.push(second);
    bytes.push(minute);
    bytes.push(hour);

    let (day, month, year) = info.build_date;
    bytes.push(day);
    bytes.push(month);
    bytes.push(year);

    bytes
}

/// Builds the payload for `QS_OBJ_DICT` records.
pub fn obj_dict_payload(address: u64, name: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + name.len() + 1);
    bytes.extend_from_slice(&address.to_le_bytes());
    push_c_string(&mut bytes, name);
    bytes
}

/// Builds the payload for `QS_FUN_DICT` records.
pub fn fun_dict_payload(address: u64, name: &str) -> Vec<u8> {
    obj_dict_payload(address, name)
}

/// Builds the payload for `QS_USR_DICT` records.
pub fn usr_dict_payload(record_id: u8, name: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1 + name.len() + 1);
    bytes.push(record_id);
    push_c_string(&mut bytes, name);
    bytes
}

/// Builds the payload for `QS_SIG_DICT` records.
pub fn sig_dict_payload(signal: u16, object: u64, name: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(2 + 8 + name.len() + 1);
    bytes.extend_from_slice(&signal.to_le_bytes());
    bytes.extend_from_slice(&object.to_le_bytes());
    push_c_string(&mut bytes, name);
    bytes
}

fn push_c_string(target: &mut Vec<u8>, value: &str) {
    target.extend_from_slice(value.as_bytes());
    target.push(0);
}
