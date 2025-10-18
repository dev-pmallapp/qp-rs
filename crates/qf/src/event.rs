//! Event and signal primitives (SRS §3.2).
//!
//! QP models *events* as lightweight messages identified by an integral
//! signal. In the original C++ code `QEvt` carries a small fixed header plus an
//! optional payload supplied by concrete applications. This module provides an
//! idiomatic Rust equivalent.

use core::fmt;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::sync::Arc;

/// Identifier for a QP signal.
///
/// Signals are globally unique numeric identifiers. The SRS recommends a
/// 16-bit range for portable deployments; we follow the same convention here.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
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
    pub const fn new(signal: Signal) -> Self {
        Self {
            signal,
            pool_id: None,
            ref_count: 1,
        }
    }

    pub fn with_pool(mut self, pool_id: u8) -> Self {
        self.pool_id = Some(pool_id);
        self
    }

    pub fn with_ref_count(mut self, ref_count: u8) -> Self {
        self.ref_count = ref_count;
        self
    }
}

/// Concrete event type with a strongly typed payload.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct Event<T = ()> {
    pub header: EventHeader,
    pub payload: T,
}

impl<T> Event<T> {
    pub fn new(signal: Signal, payload: T) -> Self {
        Self {
            header: EventHeader::new(signal),
            payload,
        }
    }

    pub fn signal(&self) -> Signal {
        self.header.signal
    }
}

impl Event<()> {
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
pub type DynPayload = Arc<dyn Any + Send + Sync>;

/// Event envelope used by the kernel to deliver events to active objects.
pub type DynEvent = Event<DynPayload>;

impl Event<DynPayload> {
    pub fn with_arc(signal: Signal, payload: DynPayload) -> Self {
        Self::new(signal, payload)
    }

    pub fn empty_dyn(signal: Signal) -> Self {
        let payload: DynPayload = Arc::new(()) as DynPayload;
        Self::with_arc(signal, payload)
    }
}
