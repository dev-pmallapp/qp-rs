//! Platform / port contract (SRS — QP-specific HAL contract).
//!
//! This module defines the small, framework-level seam that a *port* (the
//! platform runtime glue in `ports/<target>/`) implements so that application
//! code can be written **portably** — generic over the runtime rather than
//! bound to a concrete `PosixQkRuntime`, `CortexMQkRuntime`, … type.
//!
//! Per the QP-specific HAL contract, the only things QP needs from hardware are:
//! 1. **Tick source** — fires at the tick rate to advance the timer wheel
//!    ([`Runtime::tick`]).
//! 2. **Trace output** — a byte-stream sink for QS frames ([`TraceSink`]).
//! 3. **Critical section / interrupt control** — *not* abstracted here: ports
//!    already use the [`critical-section`](https://docs.rs/critical-section)
//!    crate (see `ports/esp32-*/src/interrupts.rs`). Keeping it out of `qf`
//!    leaves the core dependency-free and aligned with the embedded-hal 1.0
//!    ecosystem the HAL was migrated to.
//! 4. **Context switch** — PendSV/SVC on Cortex-M ([`ContextSwitch`]).
//!
//! ```ignore
//! use qf::port::Runtime;
//!
//! // Portable application logic — works on any port.
//! fn drive<R: Runtime>(rt: &R) {
//!     rt.tick().ok();
//!     rt.run_until_idle();
//! }
//! ```

use crate::trace::TraceHook;

/// A byte-stream trace sink that yields a QS [`TraceHook`].
///
/// Ports that own a trace transport (UART, TCP, SWO, …) implement this so the
/// kernel/active objects can be wired to it uniformly:
/// `builder.with_trace_hook(port.trace_hook())`.
pub trait TraceSink {
    /// Returns a [`TraceHook`] that writes encoded QS frames to this sink.
    fn trace_hook(&self) -> TraceHook;
}

/// Requests an asynchronous context switch.
///
/// On preemptive bare-metal targets this pends the architecture's context-switch
/// exception (PendSV on Cortex-M). On cooperative or hosted targets it is a
/// no-op — see [`NoopContextSwitch`].
pub trait ContextSwitch {
    /// Pends an asynchronous context switch, to be serviced when interrupts
    /// next allow it.
    fn request(&self);
}

/// A [`ContextSwitch`] that does nothing — for cooperative or hosted runtimes
/// where switches happen synchronously at run-to-completion boundaries.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopContextSwitch;

impl ContextSwitch for NoopContextSwitch {
    #[inline]
    fn request(&self) {}
}

/// Uniform driver API exposed by every port runtime.
///
/// Implementing this on a port's runtime struct (e.g. `PosixQkRuntime`) lets
/// application code be generic over the platform:
///
/// ```ignore
/// fn app<R: qf::port::Runtime>(rt: &R) { rt.run_until_idle(); }
/// ```
pub trait Runtime {
    /// Error returned by [`tick`](Runtime::tick) (kernel-specific time-event error).
    type TickError;

    /// Advances the time-event subsystem by one tick. Call from the platform
    /// tick ISR (or the host tick loop).
    fn tick(&self) -> Result<(), Self::TickError>;

    /// Dispatches ready work until the kernel is idle.
    fn run_until_idle(&self);

    /// Returns `true` if the kernel has work ready to dispatch.
    fn has_pending_work(&self) -> bool;
}
