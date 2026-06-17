#[cfg(feature = "qs")]
pub use qs::{TraceError, TraceHook};

/// Result of emitting a trace record.
#[cfg(feature = "qs")]
pub type TraceResult = Result<(), TraceError>;

#[cfg(not(feature = "qs"))]
use alloc::sync::Arc;

#[cfg(not(feature = "qs"))]
pub type TraceError = core::convert::Infallible;

#[cfg(not(feature = "qs"))]
pub type TraceResult = Result<(), TraceError>;

#[cfg(not(feature = "qs"))]
pub type TraceHook = Arc<dyn Fn(u8, &[u8], bool) -> TraceResult + Send + Sync>;

/// Callback invoked on a kernel context switch, receiving `(prev_prio, next_prio)`.
///
/// Priority `0` denotes the idle context, so a switch *to* idle reports
/// `next_prio == 0` and a switch *away* from idle reports `prev_prio == 0`.
/// This mirrors QP/C++'s `QF_onContextSw()` callback (introduced in QP 7.2.0 as
/// the unified hook for the QV/QK/QXK kernels); qp-rs passes priorities rather
/// than active-object pointers, consistent with its priority-keyed scheduler.
pub type ContextSwitchHook = crate::sync::Arc<dyn Fn(u8, u8) + Send + Sync>;
