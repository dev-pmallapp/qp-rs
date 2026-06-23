#![doc = r#"# qf

A clean-room, idiomatic Rust port of the [Quantum Platform Framework (QF)](https://www.state-machine.com/qpcpp/). The implementation follows the requirements in the [Software Requirements Specification](https://www.state-machine.com/qpcpp/srs-qp.html) and compiles in both `std` and `no_std` environments.

## Module Overview
- [`event`]  – Signal and event primitives (SRS §3.2).
- [`active`] – Active object state machine abstraction (SRS §3.3).
- [`kernel`] – Cooperative priority-based scheduler (SRS §3.4).
- [`time`]   – Time event services for periodic timeouts (SRS §3.5).

The crate keeps modules loosely coupled so that alternative front-ends can reuse the same primitives.
"#]
#![cfg_attr(not(feature = "std"), no_std)]

// The heap-free functional-safety build (`--no-default-features --features
// static-alloc`) links NO global allocator: `alloc` is pulled in only off the
// `static-alloc` path, or when `std` is present (host tests). Any stray heap use
// on the heap-free path is then a hard compile error — the forcing function that
// keeps the safety build allocation-free (see `docs/FUSA.md`, Phase 2).
#[cfg(any(not(feature = "static-alloc"), feature = "std"))]
extern crate alloc;

pub mod active;
pub mod dis;
pub mod equeue;
pub mod event;
pub mod event_pool;
pub mod fusa;
pub mod hsm;
pub mod qmsm;
pub mod isr;
pub mod kernel;
pub mod pool;
#[cfg(feature = "static-alloc")]
pub mod pool_arc;
pub mod port;
pub mod pubsub;
pub mod priospec;
mod sync;
pub mod time;
pub use active::{ActiveObject, ActiveObjectId, ActiveObjectRef, QActive, Q};
pub use dis::{Dis, DisAtomicU16, DisInt};
pub use equeue::{defer, flush_deferred, recall, PostStatus, QEQueue};
#[cfg(feature = "static-alloc")]
pub use equeue::StaticEQueue;
pub use event::{Event, EventHeader, Signal};
pub use event_pool::{gc, q_new, q_new_x, EventBox, PoolRegistry, POOL_REGISTRY, MAX_POOLS};
pub use fusa::{clear_error_handler, on_error, set_error_handler, ErrorHandler};
pub use hsm::{same_state, QHsm, QHsmResult, StateHandler, MAX_NEST_DEPTH, QAsm};
pub use qmsm::{QMsm, QMState, QMsmResult, QMStateHandler, same_qmstate};
pub use isr::{in_isr, isr_nesting};
pub use kernel::{Kernel, KernelBuilder, KernelConfig, QvKernel};
pub use pool::QMPool;
pub use port::{ContextSwitch, NoopContextSwitch, Runtime, TraceSink};
pub use pubsub::PubSubTable;
pub use priospec::{QPrioSpec, q_prio};
#[cfg(feature = "qs")]
pub use qs::{QsConfig, QsRecord, TraceBackend, Tracer, TracerHandle};
pub use time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo, TimerWheel};
pub use trace::{ContextSwitchHook, TraceError, TraceHook, TraceResult};
#[cfg(test)]
mod tests;
mod trace;

// ── HSM convenience macros ────────────────────────────────────────────────────

/// Declare a state transition to `$target`.
///
/// Returns `QHsmResult::Tran($target)` from a state handler.
#[macro_export]
macro_rules! q_tran {
    ($target:expr) => {
        $crate::hsm::QHsmResult::Tran($target)
    };
}

/// Delegate the event to super-state `$super`.
///
/// Returns `QHsmResult::Super($super)` from a state handler.  This is
/// the standard catch-all arm for unhandled events:
/// ```rust,ignore
/// _ => q_super!(state_a)
/// ```
#[macro_export]
macro_rules! q_super {
    ($super:expr) => {
        $crate::hsm::QHsmResult::Super($super)
    };
}

/// The event was handled; no state transition.
///
/// Returns `QHsmResult::Handled` from a state handler.
#[macro_export]
macro_rules! q_handled {
    () => {
        $crate::hsm::QHsmResult::Handled
    };
}

/// The event was intentionally ignored.
///
/// Returns `QHsmResult::Ignored` from a state handler.
#[macro_export]
macro_rules! q_ignored {
    () => {
        $crate::hsm::QHsmResult::Ignored
    };
}

/// Transition to the history of composite state `$parent`.
///
/// Returns `QHsmResult::TranHist($parent)` from a state handler.
#[macro_export]
macro_rules! q_tran_hist {
    ($parent:expr) => {
        $crate::hsm::QHsmResult::TranHist($parent)
    };
}

// ── QMsm convenience macros ───────────────────────────────────────────────────

/// Declare a QMsm state transition to `$target`.
///
/// Returns `QMsmResult::Tran($target)` from a QMsm state handler.
#[macro_export]
macro_rules! qm_tran {
    ($target:expr) => {
        $crate::qmsm::QMsmResult::Tran($target)
    };
}

/// Delegate the event to super-state `$super`.
///
/// Returns `QMsmResult::Super($super)` from a QMsm state handler.
#[macro_export]
macro_rules! qm_super {
    ($super:expr) => {
        $crate::qmsm::QMsmResult::Super($super)
    };
}

/// The event was handled; no state transition.
///
/// Returns `QMsmResult::Handled` from a QMsm state handler.
#[macro_export]
macro_rules! qm_handled {
    () => {
        $crate::qmsm::QMsmResult::Handled
    };
}

/// The event was intentionally ignored.
///
/// Returns `QMsmResult::Ignored` from a QMsm state handler.
#[macro_export]
macro_rules! qm_ignored {
    () => {
        $crate::qmsm::QMsmResult::Ignored
    };
}

/// Transition to the history of composite state `$parent`.
///
/// Returns `QMsmResult::TranHist($parent)` from a QMsm state handler.
#[macro_export]
macro_rules! qm_tran_hist {
    ($parent:expr) => {
        $crate::qmsm::QMsmResult::TranHist($parent)
    };
}

// ── ISR nesting macros ────────────────────────────────────────────────────────

/// Signal ISR entry to the QK kernel.
///
/// Must be called at the **start** of every ISR before any framework API.
/// Increments the global ISR nesting counter so the framework knows it is
/// operating in interrupt context.
///
/// Corresponds to `QK_ISR_ENTRY()` in QP/C++.
///
/// # Safety
///
/// The caller must ensure this is called exactly once per ISR entry.
#[macro_export]
macro_rules! qk_isr_entry {
    () => {
        // SAFETY: called at ISR boundary, exactly once per entry.
        unsafe { $crate::isr::isr_enter(); }
    };
}

/// Signal ISR exit to the QK kernel.
///
/// Must be called at the **end** of every ISR after all framework API calls.
/// Decrements the global ISR nesting counter.
///
/// Corresponds to `QK_ISR_EXIT()` in QP/C++.
///
/// # Safety
///
/// The caller must ensure this is called exactly once per ISR exit, and only
/// after a matching `qk_isr_entry!()`.
#[macro_export]
macro_rules! qk_isr_exit {
    () => {
        // SAFETY: called at ISR boundary, exactly once per exit.
        unsafe { $crate::isr::isr_exit(); }
    };
}
