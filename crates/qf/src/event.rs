//! Event and signal primitives (SRS §3.2).
//!
//! QP models *events* as lightweight messages identified by an integral
//! signal. In the original C++ code `QEvt` carries a small fixed header plus an
//! optional payload supplied by concrete applications. This module provides an
//! idiomatic Rust equivalent.

#[cfg(not(feature = "static-alloc"))]
use core::any::Any;
use core::fmt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "static-alloc"))]
use crate::sync::Arc;

/// Identifier for a QP signal.
///
/// Signals are globally unique numeric identifiers. The SRS recommends a
/// 16-bit range for portable deployments; we follow the same convention here.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signal(pub u16);

impl From<u16> for Signal {
    #[inline]
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SIG({:#06x})", self.0)
    }
}

/// Metadata shared by all events.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy)]
pub struct EventHeader {
    /// Event signal identifier.
    pub signal: Signal,
    /// Optional memory pool the event was obtained from (SRS §3.2.4).
    pub pool_id: Option<u8>,
    /// Reference count for garbage-collected events.
    pub ref_count: u8,
}

impl EventHeader {
    /// Creates a header for the given signal: no pool, reference count 1.
    pub const fn new(signal: Signal) -> Self {
        Self {
            signal,
            pool_id: None,
            ref_count: 1,
        }
    }

    /// Returns a copy of the header tagged with the originating pool id.
    pub fn with_pool(mut self, pool_id: u8) -> Self {
        self.pool_id = Some(pool_id);
        self
    }

    /// Returns a copy of the header with the given reference count.
    pub fn with_ref_count(mut self, ref_count: u8) -> Self {
        self.ref_count = ref_count;
        self
    }
}

/// Concrete event type with a strongly typed payload.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct Event<T = ()> {
    /// Shared event metadata (signal, pool id, refcount).
    pub header: EventHeader,
    /// Application-defined event payload.
    pub payload: T,
}

impl<T> Event<T> {
    /// Creates an event carrying `payload` for the given signal.
    pub fn new(signal: Signal, payload: T) -> Self {
        Self {
            header: EventHeader::new(signal),
            payload,
        }
    }

    /// Returns the event's signal.
    pub fn signal(&self) -> Signal {
        self.header.signal
    }
}

impl Event<()> {
    /// Creates a signal-only event with no payload.
    pub fn empty(signal: Signal) -> Self {
        Self::new(signal, ())
    }
}

impl<T: Clone> Clone for Event<T> {
    fn clone(&self) -> Self {
        Self {
            header: self.header,
            payload: self.payload.clone(),
        }
    }
}

/// Type-erased event payload suitable for heterogeneous systems.
///
/// The dynamic (default) build uses a heap `Arc<dyn Any>`; the `static-alloc`
/// build uses a heap-free, pool-backed [`PoolArc`](crate::pool_arc::PoolArc)
/// with the same shared-ownership / downcast semantics (see `docs/FUSA.md`,
/// Phase 2).
#[cfg(not(feature = "static-alloc"))]
pub type DynPayload = Arc<dyn Any + Send + Sync>;
#[cfg(feature = "static-alloc")]
pub type DynPayload = crate::pool_arc::PoolArc;

/// Event envelope used by the kernel to deliver events to active objects.
pub type DynEvent = Event<DynPayload>;

impl Event<DynPayload> {
    /// Creates a dynamic event from an already type-erased payload.
    pub fn with_arc(signal: Signal, payload: DynPayload) -> Self {
        Self::new(signal, payload)
    }

    /// Creates a signal-only dynamic event (unit payload).
    ///
    /// Allocation-free under `static-alloc` (the empty [`PoolArc`] variant).
    pub fn empty_dyn(signal: Signal) -> Self {
        #[cfg(not(feature = "static-alloc"))]
        let payload: DynPayload = Arc::new(()) as DynPayload;
        #[cfg(feature = "static-alloc")]
        let payload: DynPayload = crate::pool_arc::PoolArc::empty();
        Self::with_arc(signal, payload)
    }

    /// Creates a dynamic event carrying a typed `payload`.
    ///
    /// Portable across both allocation models: heap `Arc` on the default build,
    /// a pool-backed [`PoolArc`](crate::pool_arc::PoolArc) under `static-alloc`.
    /// Prefer this over `with_arc(Arc::new(..))` in code that must build for the
    /// functional-safety (heap-free) target.
    pub fn with_payload<T: core::any::Any + Send + Sync>(signal: Signal, payload: T) -> Self {
        #[cfg(not(feature = "static-alloc"))]
        let payload: DynPayload = Arc::new(payload);
        #[cfg(feature = "static-alloc")]
        let payload: DynPayload = crate::pool_arc::PoolArc::from_value(payload);
        Self::with_arc(signal, payload)
    }
}
