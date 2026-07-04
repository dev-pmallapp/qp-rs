//! Quantum Platform RTOS framework — unified facade.
//!
//! This crate re-exports the constituent `qp-rs` crates as named submodules so
//! that downstream projects need only a single dependency:
//!
//! ```toml
//! [dependencies]
//! qp-rs = { path = "vendor/qp-rs/crates/qp-rs", default-features = false,
//!           features = ["qk", "qs", "std"] }
//! ```
//!
//! # Module layout
//!
//! | Module | Content | Feature gate |
//! |--------|---------|--------------|
//! | [`qf`] | Active objects, events, cooperative kernel, time events | always |
//! | [`qk`] | Preemptive single-stack kernel | `qk` |
//! | [`qxk`] | Dual-mode kernel with blocking threads | `qxk` |
//! | [`qs`] | QS binary tracing protocol | `qs` |
//!
//! # Quick start
//!
//! ```rust
//! use qp_rs::prelude::*;
//! use qp_rs::qk::QkKernel;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

// Re-export each constituent crate as a public submodule.
// Downstream code: `use qp_rs::qf::QHsm;`
pub use qf;

#[cfg(feature = "qk")]
pub use qk;

#[cfg(feature = "qxk")]
pub use qxk;

#[cfg(feature = "qs")]
pub use qs;

#[cfg(feature = "comms")]
pub use comms;

#[cfg(feature = "hal")]
pub use hal;

/// Platform / port contract ([`Runtime`], [`TraceSink`], [`ContextSwitch`]).
///
/// Implemented by the thin per-target port crates so application code can be
/// written generically over the runtime. See [`qf::port`].
pub use qf::port;

/// Commonly used imports — `use qp_rs::prelude::*;` in application code.
pub mod prelude {
    pub use qf::{
        // Active objects
        ActiveObject,
        QActive,
        Q,
        // Events
        Event,
        Signal,
        // State machines
        QAsm,
        QHsm,
        QMsm,
        SameState,
        // Cooperative kernel (QP/C++ QV equivalent; `Kernel` is a back-compat alias)
        QvKernel,
        Kernel,
        KernelConfig,
        // Time events
        TimeEvent,
        TimerWheel,
        // Priority spec
        QPrioSpec,
        q_prio,
        // Tracing hook (always available; backend is feature-gated)
        TraceHook,
        // Context-switch hook (QP/C++ QF_onContextSw equivalent)
        ContextSwitchHook,
        // Platform / port contract for portable application code
        ContextSwitch,
        Runtime,
        TraceSink,
    };

    #[allow(deprecated)]
    pub use qf::{same_state, same_qmstate};

    #[cfg(feature = "qk")]
    pub use qk::{QkKernel, QkKernelBuilder, QkTimerWheel};

    #[cfg(feature = "qxk")]
    pub use qxk::{
        QxkKernel,
        QxkKernelBuilder,
        QxkScheduler,
        Semaphore,
        MutexPrim,
        MessageQueue,
        CondVar,
    };

    #[cfg(feature = "qs")]
    pub use qs::{QsConfig, Tracer, TraceBackend};

    #[cfg(feature = "comms")]
    pub use comms::{RfStack, RfStackAO, FotaDriver, FotaStatus};
}
