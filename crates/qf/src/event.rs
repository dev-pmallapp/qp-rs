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
///
/// Like a hardware `Pin` handle, `Signal` is a first-class opaque object, not
/// a raw integer newtype: its field is private and the only ways to obtain
/// one are [`Signal::reserved`] (framework-internal, `0..Q_USER_SIG`) and
/// [`Signal::user`] (application-defined, `Q_USER_SIG..`). Both are `const
/// fn`s that panic — a compile error, when used in a `const` item — if the
/// value falls on the wrong side of that boundary. This exists because
/// nothing previously stopped an application-level `SIG_*` constant from
/// silently drifting onto the numeric value of an unrelated one defined
/// elsewhere; see [`SignalBlock`]/[`assert_no_overlap`] for checking
/// non-overlap *between* whole blocks of user signals too.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signal(u16);

/// First signal value legal for application/user code — matches
/// [`crate::hsm::reserved::Q_USER_SIG`]'s numeric value. Duplicated here
/// (rather than depended on) because `event` is a lower module than `hsm`;
/// [`crate::hsm::reserved::Q_USER_SIG`] is defined in terms of
/// [`Signal::user`] using this same constant, so the two can never drift
/// apart.
const FIRST_USER_SIG: u16 = 4;

impl Signal {
    /// Constructs a framework-reserved signal (`Q_EMPTY_SIG`/`Q_ENTRY_SIG`/
    /// `Q_EXIT_SIG`/`Q_INIT_SIG`). Framework-internal use only — application
    /// code should never need this; use [`Signal::user`] instead.
    ///
    /// Panics (a compile error in `const` context) if `n >= Q_USER_SIG`.
    #[must_use]
    pub const fn reserved(n: u16) -> Self {
        assert!(
            n < FIRST_USER_SIG,
            "Signal::reserved() value must be < Q_USER_SIG (4) — use Signal::user() for application signals"
        );
        Self(n)
    }

    /// Constructs an application-defined signal.
    ///
    /// Panics (a compile error in `const` context) if `n < Q_USER_SIG`,
    /// i.e. if it collides with a framework-reserved signal.
    #[must_use]
    pub const fn user(n: u16) -> Self {
        assert!(
            n >= FIRST_USER_SIG,
            "Signal::user() value must be >= Q_USER_SIG (4) — values 0-3 are reserved for the framework"
        );
        Self(n)
    }

    /// The raw numeric value, for tracing/logging/wire encoding.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Constructs a `Signal` from an arbitrary runtime value with no range
    /// check.
    ///
    /// For wire-decoded or test-harness values where any `u16` may
    /// legitimately need representing — e.g. QSpy's `Event` RX command
    /// replaying a signal number it observed, or a test injecting a
    /// framework-reserved signal on purpose. Prefer [`Signal::user`] /
    /// [`Signal::reserved`] for any compile-time-known constant; those are
    /// what `assert_no_overlap` actually protects.
    #[must_use]
    pub const fn from_raw(n: u16) -> Self {
        Self(n)
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SIG({:#06x})", self.0)
    }
}

/// A named, contiguous range of user-signal values reserved by one module —
/// metadata for [`assert_no_overlap`], not a `Signal` itself.
///
/// Each independent signal-numbering scheme in the tree (the `comms` crate's
/// `RF_*` block, an application's per-role signal blocks, ...) should declare
/// one of these and list it in a single, shared `assert_no_overlap` call, so
/// a new block can never silently overlap an existing one the way
/// `SIG_PAIRING_POLL_TICK` once drifted onto `SIG_FOTA_APPLY_ACCEPTED` in
/// `swm-rs` — that collision compiled cleanly because the two were tracked in
/// separate, unrelated registries.
#[derive(Debug, Clone, Copy)]
pub struct SignalBlock {
    /// Human-readable name, shown in overlap-assertion context by callers.
    pub name: &'static str,
    /// First signal value in the block (inclusive).
    pub base: u16,
    /// Number of signal values the block reserves.
    pub len: u16,
}

impl SignalBlock {
    /// Declares a block of `len` signal values starting at `base`.
    ///
    /// Panics (a compile error in `const` context) if `base` falls in the
    /// framework-reserved range, or if `len` is zero.
    #[must_use]
    pub const fn new(name: &'static str, base: u16, len: u16) -> Self {
        assert!(
            base >= FIRST_USER_SIG,
            "SignalBlock::base must be >= Q_USER_SIG (4)"
        );
        assert!(len > 0, "SignalBlock::len must be nonzero");
        Self { name, base, len }
    }

    /// First value past the end of the block (exclusive).
    #[must_use]
    pub const fn end(&self) -> u16 {
        self.base + self.len
    }
}

/// Compile-time check that no two blocks in `blocks` overlap.
///
/// Intended to be called from a single, shared `const _: () =
/// assert_no_overlap(&[...]);` item listing every [`SignalBlock`] in the
/// tree, so adding a new block that overlaps an existing one fails the
/// build instead of compiling silently.
pub const fn assert_no_overlap(blocks: &[SignalBlock]) {
    let mut i = 0;
    while i < blocks.len() {
        let mut j = i + 1;
        while j < blocks.len() {
            let a = &blocks[i];
            let b = &blocks[j];
            let overlaps = a.base < b.end() && b.base < a.end();
            assert!(!overlaps, "two SignalBlocks overlap — see assert_no_overlap's caller");
            j += 1;
        }
        i += 1;
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
