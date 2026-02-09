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
pub mod kernel;
mod sync;
pub mod time;
pub use active::{ActiveObject, ActiveObjectId, ActiveObjectRef};
pub use event::{Event, EventHeader, Signal};
pub use kernel::{Kernel, KernelBuilder, KernelConfig};
#[cfg(feature = "qs")]
pub use qs::{QsConfig, QsRecord, TraceBackend, Tracer, TracerHandle};
pub use time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo, TimerWheel};
pub use trace::{TraceError, TraceHook, TraceResult};
#[cfg(test)]
mod tests;
mod trace;
