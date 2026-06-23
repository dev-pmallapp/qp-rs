#[cfg(feature = "qs")]
pub use qs::{TraceError, TraceHook};

/// Result of emitting a trace record.
#[cfg(feature = "qs")]
pub type TraceResult = Result<(), TraceError>;

#[cfg(all(not(feature = "qs"), not(feature = "static-alloc")))]
use alloc::sync::Arc;

#[cfg(not(feature = "qs"))]
pub type TraceError = core::convert::Infallible;

#[cfg(not(feature = "qs"))]
pub type TraceResult = Result<(), TraceError>;

// Trace hook (when `qs` is off). The heap-free `static-alloc` build links no
// allocator, so the hook is a `&'static` function object rather than an `Arc`
// (see `docs/FUSA.md`, Phase 2). Both are `Clone`/`Copy`, so kernel code is
// uniform. (With `qs` on, the hook type comes from the `qs` crate.)
#[cfg(all(not(feature = "qs"), not(feature = "static-alloc")))]
pub type TraceHook = Arc<dyn Fn(u8, &[u8], bool) -> TraceResult + Send + Sync>;
#[cfg(all(not(feature = "qs"), feature = "static-alloc"))]
pub type TraceHook = &'static (dyn Fn(u8, &[u8], bool) -> TraceResult + Send + Sync);

/// Callback invoked on a kernel context switch, receiving `(prev_prio, next_prio)`.
///
/// Priority `0` denotes the idle context, so a switch *to* idle reports
/// `next_prio == 0` and a switch *away* from idle reports `prev_prio == 0`.
/// This mirrors QP/C++'s `QF_onContextSw()` callback (introduced in QP 7.2.0 as
/// the unified hook for the QV/QK/QXK kernels); qp-rs passes priorities rather
/// than active-object pointers, consistent with its priority-keyed scheduler.
///
/// Heap-free under `static-alloc` (a `&'static` function object instead of an
/// `Arc`).
#[cfg(not(feature = "static-alloc"))]
pub type ContextSwitchHook = crate::sync::Arc<dyn Fn(u8, u8) + Send + Sync>;
#[cfg(feature = "static-alloc")]
pub type ContextSwitchHook = &'static (dyn Fn(u8, u8) + Send + Sync);
