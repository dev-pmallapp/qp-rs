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

extern crate alloc;

pub mod active;
pub mod event;
pub mod event_pool;
pub mod hsm;
pub mod kernel;
pub mod pool;
mod sync;
pub mod time;
pub use active::{ActiveObject, ActiveObjectId, ActiveObjectRef};
pub use event::{Event, EventHeader, Signal};
pub use event_pool::{gc, q_new, q_new_x, EventBox, PoolRegistry, POOL_REGISTRY, MAX_POOLS};
pub use hsm::{same_state, QHsm, QHsmResult, StateHandler, MAX_NEST_DEPTH};
pub use kernel::{Kernel, KernelBuilder, KernelConfig};
pub use pool::QMPool;
#[cfg(feature = "qs")]
pub use qs::{QsConfig, QsRecord, TraceBackend, Tracer, TracerHandle};
pub use time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo, TimerWheel};
pub use trace::{TraceError, TraceHook, TraceResult};
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
