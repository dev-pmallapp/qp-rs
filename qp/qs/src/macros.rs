//! QS Tracing Macros
//!
//! High-level macros for all predefined QS record types

// ============================================================================
// State Machine Macros
// ============================================================================

/// Trace state entry
#[macro_export]
macro_rules! qs_sm_entry {
    ($obj:expr, $state:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_STATE_ENTRY, 0) {
            $crate::obj_ptr($obj);
            $crate::fun_ptr($state);
            $crate::end();
        }
    };
}

/// Trace state exit
#[macro_export]
macro_rules! qs_sm_exit {
    ($obj:expr, $state:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_STATE_EXIT, 0) {
            $crate::obj_ptr($obj);
            $crate::fun_ptr($state);
            $crate::end();
        }
    };
}

/// Trace state machine initialization  
/// Note: QS_QEP_STATE_INIT only sends obj + target (2 pointers), not source
#[macro_export]
macro_rules! qs_sm_init {
    ($obj:expr, $initial:expr, $target:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_STATE_INIT, 0) {
            $crate::obj_ptr($obj);
            // Skip $initial - STATE_INIT only sends target, not source
            $crate::fun_ptr($target);
            $crate::end();
        }
    };
}

/// Trace initial transition
#[macro_export]
macro_rules! qs_init_tran {
    ($obj:expr, $target:expr) => {
        if $crate::begin($crate::QSRecordType::QS_INIT_TRAN, 0) {
            $crate::obj_ptr($obj);
            $crate::fun_ptr($target);
            $crate::end();
        }
    };
}

/// Trace internal transition
#[macro_export]
macro_rules! qs_intern_tran {
    ($obj:expr, $source:expr) => {
        if $crate::begin($crate::QSRecordType::QS_INTERN_TRAN, 0) {
            $crate::obj_ptr($obj);
            $crate::fun_ptr($source);
            $crate::end();
        }
    };
}

/// Trace state transition
#[macro_export]
macro_rules! qs_sm_tran {
    ($obj:expr, $source:expr, $target:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_TRAN, 0) {
            $crate::obj_ptr($obj as *const _ as *const u8 as usize);
            $crate::fun_ptr($source);
            $crate::fun_ptr($target);
            $crate::end();
        }
    };
}

/// Trace ignored event
#[macro_export]
macro_rules! qs_ignored {
    ($obj:expr, $state:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_IGNORED, 0) {
            $crate::obj_ptr($obj as *const _ as *const u8 as usize);
            $crate::fun_ptr($state);
            $crate::end();
        }
    };
}

/// Trace event dispatch
#[macro_export]
macro_rules! qs_sm_dispatch {
    ($obj:expr, $state:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QEP_DISPATCH, 0) {
            $crate::obj_ptr($obj as *const _ as *const u8 as usize);
            $crate::fun_ptr($state);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

// ============================================================================
// Active Object Macros
// ============================================================================

/// Trace event post
#[macro_export]
macro_rules! qs_ao_post {
    ($sender:expr, $receiver:expr, $evt:expr, $margin:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_POST, 0) {
            $crate::obj_ptr($sender);
            $crate::obj_ptr($receiver);
            $crate::evt_ptr($evt);
            $crate::queue_ctr($margin);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

/// Trace LIFO post
#[macro_export]
macro_rules! qs_ao_post_lifo {
    ($sender:expr, $receiver:expr, $evt:expr, $margin:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_POST_LIFO, 0) {
            $crate::obj_ptr($sender);
            $crate::obj_ptr($receiver);
            $crate::evt_ptr($evt);
            $crate::queue_ctr($margin);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

/// Trace event get
#[macro_export]
macro_rules! qs_ao_get {
    ($obj:expr, $evt:expr, $qlen:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_GET, 0) {
            $crate::obj_ptr($obj);
            $crate::evt_ptr($evt);
            $crate::queue_ctr($qlen);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

/// Trace get last event
#[macro_export]
macro_rules! qs_ao_get_last {
    ($obj:expr, $evt:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_GET_LAST, 0) {
            $crate::obj_ptr($obj);
            $crate::evt_ptr($evt);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

/// Trace event subscription
#[macro_export]
macro_rules! qs_ao_subscribe {
    ($obj:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_SUBSCRIBE, 0) {
            $crate::obj_ptr($obj);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

/// Trace event unsubscription
#[macro_export]
macro_rules! qs_ao_unsubscribe {
    ($obj:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_AO_UNSUBSCRIBE, 0) {
            $crate::obj_ptr($obj);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

// ============================================================================
// Memory Pool Macros
// ============================================================================

/// Trace memory pool get
#[macro_export]
macro_rules! qs_mp_get {
    ($pool:expr, $block:expr, $nfree:expr) => {
        if $crate::begin($crate::QSRecordType::QS_MP_GET, 0) {
            $crate::obj_ptr($pool);
            $crate::obj_ptr($block);
            $crate::pool_ctr($nfree);
            $crate::end();
        }
    };
}

/// Trace memory pool put
#[macro_export]
macro_rules! qs_mp_put {
    ($pool:expr, $block:expr, $nfree:expr) => {
        if $crate::begin($crate::QSRecordType::QS_MP_PUT, 0) {
            $crate::obj_ptr($pool);
            $crate::obj_ptr($block);
            $crate::pool_ctr($nfree);
            $crate::end();
        }
    };
}

// ============================================================================
// Time Event Macros
// ============================================================================

/// Trace QF tick
#[macro_export]
macro_rules! qs_qf_tick {
    ($tick_rate:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QF_TICK, 0) {
            $crate::u8($tick_rate);
            $crate::end();
        }
    };
}

/// Trace time event arming
#[macro_export]
macro_rules! qs_te_arm {
    ($tevt:expr, $obj:expr, $nTicks:expr, $interval:expr) => {
        if $crate::begin($crate::QSRecordType::QS_TE_ARM, 0) {
            $crate::obj_ptr($tevt);
            $crate::obj_ptr($obj);
            $crate::te_ctr($nTicks);
            $crate::te_ctr($interval);
            $crate::end();
        }
    };
}

/// Trace time event auto-disarm
#[macro_export]
macro_rules! qs_te_auto_disarm {
    ($tevt:expr, $obj:expr) => {
        if $crate::begin($crate::QSRecordType::QS_TE_AUTO_DISARM, 0) {
            $crate::obj_ptr($tevt);
            $crate::obj_ptr($obj);
            $crate::end();
        }
    };
}

/// Trace time event disarm attempt
#[macro_export]
macro_rules! qs_te_disarm_attempt {
    ($tevt:expr, $obj:expr) => {
        if $crate::begin($crate::QSRecordType::QS_TE_DISARM_ATTEMPT, 0) {
            $crate::obj_ptr($tevt);
            $crate::obj_ptr($obj);
            $crate::end();
        }
    };
}

/// Trace time event disarm
#[macro_export]
macro_rules! qs_te_disarm {
    ($tevt:expr, $obj:expr, $was_armed:expr) => {
        if $crate::begin($crate::QSRecordType::QS_TE_DISARM, 0) {
            $crate::obj_ptr($tevt);
            $crate::obj_ptr($obj);
            $crate::u8($was_armed as u8);
            $crate::end();
        }
    };
}

/// Trace time event post
#[macro_export]
macro_rules! qs_te_post {
    ($tevt:expr, $obj:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_TE_POST, 0) {
            $crate::obj_ptr($tevt);
            $crate::obj_ptr($obj);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

// ============================================================================
// QF Event Management Macros
// ============================================================================

/// Trace event allocation
#[macro_export]
macro_rules! qs_qf_new {
    ($evt:expr, $sig:expr, $margin:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QF_NEW, 0) {
            $crate::evt_ptr($evt);
            $crate::signal($sig);
            $crate::pool_ctr($margin);
            $crate::end();
        }
    };
}

/// Trace event garbage collection
#[macro_export]
macro_rules! qs_qf_gc {
    ($evt:expr, $sig:expr, $ref_ctr:expr) => {
        if $crate::begin($crate::QSRecordType::QS_QF_GC, 0) {
            $crate::evt_ptr($evt);
            $crate::signal($sig);
            $crate::u8($ref_ctr);
            $crate::end();
        }
    };
}

/// Trace event publish
#[macro_export]
macro_rules! qs_publish {
    ($evt:expr, $sig:expr) => {
        if $crate::begin($crate::QSRecordType::QS_PUBLISH, 0) {
            $crate::evt_ptr($evt);
            $crate::signal($sig);
            $crate::end();
        }
    };
}

// ============================================================================
// Application-Specific Macros
// ============================================================================

/// Begin application-specific record with format byte
#[macro_export]
macro_rules! qs_begin_id {
    ($rec:expr, $qs_id:expr) => {
        $crate::begin($rec, $qs_id)
    };
}

/// End application-specific record
#[macro_export]
macro_rules! qs_end {
    () => {
        $crate::end()
    };
}

/// Output data with format byte for app-specific records
#[macro_export]
macro_rules! qs_app_u8 {
    ($width:expr, $value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::U8, $width);
        $crate::u8(fmt.0);
        $crate::u8($value);
    }};
}

/// Output i8 with format byte
#[macro_export]
macro_rules! qs_app_i8 {
    ($width:expr, $value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::I8, $width);
        $crate::u8(fmt.0);
        $crate::i8($value);
    }};
}

/// Output u16 with format byte
#[macro_export]
macro_rules! qs_app_u16 {
    ($width:expr, $value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::U16, $width);
        $crate::u8(fmt.0);
        $crate::u16($value);
    }};
}

/// Output i16 with format byte
#[macro_export]
macro_rules! qs_app_i16 {
    ($width:expr, $value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::I16, $width);
        $crate::u8(fmt.0);
        $crate::i16($value);
    }};
}

/// Output u32 with format byte
#[macro_export]
macro_rules! qs_app_u32 {
    ($width:expr, $value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::U32, $width);
        $crate::u8(fmt.0);
        $crate::u32($value);
    }};
}

/// Output string with format byte
#[macro_export]
macro_rules! qs_app_str {
    ($value:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::STR, 0);
        $crate::u8(fmt.0);
        $crate::str($value);
    }};
}

/// Output memory block with format byte
#[macro_export]
macro_rules! qs_app_mem {
    ($data:expr, $len:expr) => {{
        let fmt = $crate::FormatByte::new($crate::FormatByte::MEM, 0);
        $crate::u8(fmt.0);
        $crate::mem($data, $len);
    }};
}
