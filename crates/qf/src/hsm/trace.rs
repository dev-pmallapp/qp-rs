//! Centralized software tracing (QS/QSPY) emission helpers for the HSM event processors.

use crate::event::Signal;
use crate::trace::TraceHook;

// QS record IDs for QEP events. Matches QP/C++ v8.x canonical values.
const QS_QEP_STATE_ENTRY:  u8 = 1;
const QS_QEP_STATE_EXIT:   u8 = 2;
const QS_QEP_STATE_INIT:   u8 = 3;
const QS_QEP_INIT_TRAN:    u8 = 4;
const QS_QEP_INTERN_TRAN:  u8 = 5;
const QS_QEP_TRAN:         u8 = 6;
const QS_QEP_IGNORED:      u8 = 7;
const QS_QEP_DISPATCH:     u8 = 8;
#[allow(dead_code)]
const QS_QEP_UNHANDLED:    u8 = 9;
const QS_QEP_TRAN_HIST:    u8 = 55;

const PTR_SIZE: usize = core::mem::size_of::<usize>();

pub fn emit_state_entry(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_ENTRY, &state_ptr.to_le_bytes(), false);
}

pub fn emit_state_exit(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_EXIT, &state_ptr.to_le_bytes(), false);
}

pub fn emit_state_init(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_STATE_INIT, &state_ptr.to_le_bytes(), false);
}

pub fn emit_init_tran(hook: &TraceHook, state_ptr: usize) {
    let _ = hook(QS_QEP_INIT_TRAN, &state_ptr.to_le_bytes(), false);
}

pub fn emit_dispatch(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_DISPATCH, &buf, true);
}

pub fn emit_intern_tran(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_INTERN_TRAN, &buf, true);
}

pub fn emit_ignored(hook: &TraceHook, sig: Signal, state_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..].copy_from_slice(&state_ptr.to_le_bytes());
    let _ = hook(QS_QEP_IGNORED, &buf, true);
}

pub fn emit_tran(hook: &TraceHook, sig: Signal, source_ptr: usize, target_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE * 2];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..2 + PTR_SIZE].copy_from_slice(&source_ptr.to_le_bytes());
    buf[2 + PTR_SIZE..].copy_from_slice(&target_ptr.to_le_bytes());
    let _ = hook(QS_QEP_TRAN, &buf, true);
}

pub fn emit_tran_hist(hook: &TraceHook, sig: Signal, source_ptr: usize, target_ptr: usize) {
    let mut buf = [0u8; 2 + PTR_SIZE * 2];
    buf[0..2].copy_from_slice(&sig.0.to_le_bytes());
    buf[2..2 + PTR_SIZE].copy_from_slice(&source_ptr.to_le_bytes());
    buf[2 + PTR_SIZE..].copy_from_slice(&target_ptr.to_le_bytes());
    let _ = hook(QS_QEP_TRAN_HIST, &buf, true);
}
