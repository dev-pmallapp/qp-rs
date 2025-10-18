//! # qf
//!
//! A clean-room, idiomatic Rust port of the [Quantum Platform Framework
//! (QF)](https://www.state-machine.com/qpcpp/). The implementation follows the
//! requirements in the [Software Requirements Specification]
//! (https://www.state-machine.com/qpcpp/srs-qp.html) and targets `x86_64` with
//! the standard library enabled.
//!
//! ## Module Overview
//! - [`event`]  – Signal and event primitives (SRS §3.2).
//! - [`active`] – Active object state machine abstraction (SRS §3.3).
//! - [`kernel`] – Cooperative priority-based scheduler (SRS §3.4).
//! - [`time`]   – Time event services for periodic timeouts (SRS §3.5).
//!
//! The crate keeps modules loosely coupled so that alternative front-ends can
//! reuse the same primitives.

pub mod active;
pub mod event;
pub mod kernel;
pub mod time;
pub use active::{ActiveObject, ActiveObjectId, ActiveObjectRef};
pub use event::{Event, EventHeader, Signal};
pub use kernel::{Kernel, KernelBuilder, KernelConfig};
pub use qs::{QsConfig, QsRecord, TraceBackend, TraceError, TraceHook, Tracer, TracerHandle};
pub use time::{TimeEvent, TimerWheel};
#[cfg(test)]
mod tests;
