//! Active object abstraction (SRS §3.3).
//!
//! Active objects encapsulate state machines with event queues and execute in
//! priority order under the control of the QP kernel.

#[cfg(all(feature = "std", not(feature = "static-alloc")))]
use std::collections::VecDeque;

#[cfg(all(not(feature = "std"), not(feature = "static-alloc")))]
use alloc::collections::VecDeque;

use crate::sync::{Arc, Mutex};
use crate::trace::{TraceError, TraceHook};

use crate::event::{DynEvent, Signal};

/// Unique identifier for an active object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ActiveObjectId(pub u8);

impl ActiveObjectId {
    /// Creates an active-object id from a raw `u8`.
    pub const fn new(id: u8) -> Self {
        Self(id)
    }
}

/// Per-dispatch context passed to state handlers.
pub struct ActiveContext {
    id: ActiveObjectId,
    trace: Option<TraceHook>,
}

impl ActiveContext {
    /// Creates a dispatch context for the given active object and trace hook.
    pub fn new(id: ActiveObjectId, trace: Option<TraceHook>) -> Self {
        Self { id, trace }
    }

    /// Returns the id of the active object this context belongs to.
    pub fn id(&self) -> ActiveObjectId {
        self.id
    }

    /// Emits a QS trace record (with timestamp) via the context's trace hook.
    pub fn emit_trace(&self, record_type: u8, payload: &[u8]) -> Result<(), TraceError> {
        self.emit_trace_with_timestamp(record_type, payload, true)
    }

    /// Emits a QS trace record, choosing whether to include a timestamp.
    pub fn emit_trace_with_timestamp(
        &self,
        record_type: u8,
        payload: &[u8],
        with_timestamp: bool,
    ) -> Result<(), TraceError> {
        if let Some(hook) = &self.trace {
            hook(record_type, payload, with_timestamp)
        } else {
            Ok(())
        }
    }

    /// Returns a clone of the context's trace hook, if any.
    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }
}

/// Trait implemented by application state machines.
pub trait ActiveBehavior: Send + 'static {
    /// Called once when the active object starts (top-most initial transition).
    fn on_start(&mut self, ctx: &mut ActiveContext);
    /// Called for each event dispatched to the active object.
    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent);
}

/// Object-safe interface used by the kernel.
pub trait ActiveRunnable: Send + Sync {
    /// Returns this active object's id.
    fn id(&self) -> ActiveObjectId;
    /// Returns this active object's priority.
    fn priority(&self) -> u8;
    /// Starts the active object, installing the given trace hook.
    fn start(&self, trace: Option<TraceHook>);
    /// Dispatches at most one queued event; returns `true` if one was handled.
    fn dispatch_one(&self) -> bool;
    /// Posts an event to the back of this active object's queue (FIFO).
    fn post(&self, event: DynEvent);
    /// Post an event LIFO (to the front of this AO's queue).
    ///
    /// Used by `recall()` to give a recalled event priority over pending events.
    fn post_lifo(&self, event: DynEvent);
    /// Returns `true` if this active object has queued events.
    fn has_events(&self) -> bool;
}

/// Default per-active-object queue capacity for the `static-alloc` (heap-free)
/// build.
///
/// Every active object's queue shares this fixed inline capacity — size it for
/// the deepest queue in the system. Overflowing it is a functional-safety fault
/// (the queue was undersized for the worst case), routed through
/// [`fusa::on_error`](crate::fusa::on_error), mirroring QP/C's queue-overflow
/// assertion. The dynamic (default) build keeps an unbounded `VecDeque` and has
/// no such limit.
#[cfg(feature = "static-alloc")]
pub const AO_QUEUE_CAPACITY: usize = 16;

#[cfg(not(feature = "static-alloc"))]
type EventBuf = VecDeque<DynEvent>;
#[cfg(feature = "static-alloc")]
type EventBuf = heapless::Deque<DynEvent, AO_QUEUE_CAPACITY>;

/// Event queue with an occupancy high-water mark.
///
/// Tracks the maximum number of events ever queued simultaneously — the
/// meaningful portable metric for sizing a bounded queue on a constrained
/// target. Under the `static-alloc` feature the storage is a fixed-capacity,
/// heap-free [`heapless::Deque`]; otherwise it is an unbounded `VecDeque`.
struct EventQueue {
    buf: EventBuf,
    high_watermark: usize,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            #[cfg(not(feature = "static-alloc"))]
            buf: VecDeque::new(),
            #[cfg(feature = "static-alloc")]
            buf: heapless::Deque::new(),
            high_watermark: 0,
        }
    }

    /// Records the current length as the new high-water mark if it is larger.
    fn touch_watermark(&mut self) {
        if self.buf.len() > self.high_watermark {
            self.high_watermark = self.buf.len();
        }
    }

    /// Enqueue FIFO. Updates the watermark. Faults on overflow under
    /// `static-alloc`.
    fn push_back(&mut self, event: DynEvent) {
        #[cfg(not(feature = "static-alloc"))]
        self.buf.push_back(event);
        #[cfg(feature = "static-alloc")]
        if self.buf.push_back(event).is_err() {
            crate::fusa::on_error(module_path!(), line!());
        }
        self.touch_watermark();
    }

    /// Enqueue LIFO (front). Updates the watermark. Faults on overflow under
    /// `static-alloc`.
    fn push_front(&mut self, event: DynEvent) {
        #[cfg(not(feature = "static-alloc"))]
        self.buf.push_front(event);
        #[cfg(feature = "static-alloc")]
        if self.buf.push_front(event).is_err() {
            crate::fusa::on_error(module_path!(), line!());
        }
        self.touch_watermark();
    }

    #[inline]
    fn pop_front(&mut self) -> Option<DynEvent> {
        self.buf.pop_front()
    }

    #[inline]
    fn len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// Concrete active object implementation for a specific behavior.
pub struct ActiveObject<B: ActiveBehavior> {
    id: ActiveObjectId,
    priority: u8,
    queue: Mutex<EventQueue>,
    behavior: Mutex<B>,
    trace_hook: Mutex<Option<TraceHook>>,
}

impl<B: ActiveBehavior> ActiveObject<B> {
    /// Creates an active object with the given id, priority, and behavior,
    /// returning it wrapped in an [`Arc`] ready for kernel registration.
    pub fn new(id: ActiveObjectId, priority: u8, behavior: B) -> Arc<Self> {
        Arc::new(Self {
            id,
            priority,
            queue: Mutex::new(EventQueue::new()),
            behavior: Mutex::new(behavior),
            trace_hook: Mutex::new(None),
        })
    }

    /// Borrow the behavior under its lock and apply `f`.
    ///
    /// Used by the kernel host to read active-object state snapshots
    /// (e.g. current HSM mode, last reading) without exposing the mutex.
    pub fn with_behavior<R>(&self, f: impl FnOnce(&B) -> R) -> R {
        let guard = self.behavior.lock();
        f(&*guard)
    }

    /// Mutably borrow the behavior under its lock and apply `f`.
    pub fn with_behavior_mut<R>(&self, f: impl FnOnce(&mut B) -> R) -> R {
        let mut guard = self.behavior.lock();
        f(&mut *guard)
    }

    fn pop_event(&self) -> Option<DynEvent> {
        let mut queue = self.queue.lock();
        queue.pop_front()
    }

    /// Number of events currently waiting in this active object's queue.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().len()
    }

    /// Maximum number of events ever queued simultaneously (occupancy
    /// high-water mark). Sticky once observed; never decreases.
    ///
    /// This is the unbounded-queue analog of QP/C++ `QActive::getQueueMin()`:
    /// use it to size a bounded queue when porting to a constrained target.
    pub fn queue_high_watermark(&self) -> usize {
        self.queue.lock().high_watermark
    }

    fn build_context(&self) -> ActiveContext {
        let trace = self.trace_hook.lock().clone();
        ActiveContext::new(self.id, trace)
    }
}

/// Erase a typed active-object `Arc` to the [`ActiveRunnable`] trait object.
///
/// This is a convenience wrapper around the trait-object coercion
/// `Arc<ActiveObject<B>> as Arc<dyn ActiveRunnable>`.  It exists because
/// some `Arc` backends (e.g. `portable_atomic_util::Arc`) do not implement
/// `CoerceUnsized` on stable Rust.
pub fn arc_as_runnable<B: ActiveBehavior>(ao: Arc<ActiveObject<B>>) -> ActiveObjectRef {
    ao as ActiveObjectRef
}

impl<B: ActiveBehavior> ActiveRunnable for ActiveObject<B> {
    fn id(&self) -> ActiveObjectId {
        self.id
    }

    fn priority(&self) -> u8 {
        self.priority
    }

    fn start(&self, trace: Option<TraceHook>) {
        *self.trace_hook.lock() = trace.clone();
        let mut behavior = self.behavior.lock();
        let mut ctx = ActiveContext::new(self.id, trace);
        behavior.on_start(&mut ctx);
    }

    fn dispatch_one(&self) -> bool {
        if let Some(event) = self.pop_event() {
            let mut behavior = self.behavior.lock();
            let mut ctx = self.build_context();
            behavior.on_event(&mut ctx, event);
            true
        } else {
            false
        }
    }

    fn post(&self, event: DynEvent) {
        self.queue.lock().push_back(event);
    }

    fn post_lifo(&self, event: DynEvent) {
        self.queue.lock().push_front(event);
    }

    fn has_events(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

/// Type-erased, shareable handle to an active object used by the kernel registry.
pub type ActiveObjectRef = Arc<dyn ActiveRunnable>;

/// Helper builder for typed active objects.
pub fn new_active_object<B: ActiveBehavior>(
    id: ActiveObjectId,
    priority: u8,
    behavior: B,
) -> ActiveObjectRef {
    ActiveObject::new(id, priority, behavior) as ActiveObjectRef
}

/// Convenience behavior for static state machines that only react to signals.
pub trait SignalHandler: Send + 'static {
    /// Optional start hook; defaults to doing nothing.
    fn on_start(&mut self, _ctx: &mut ActiveContext) {}
    /// Handles a single incoming signal.
    fn handle_signal(&mut self, signal: Signal, ctx: &mut ActiveContext);
}

impl<T: SignalHandler> ActiveBehavior for T {
    fn on_start(&mut self, ctx: &mut ActiveContext) {
        SignalHandler::on_start(self, ctx);
    }

    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent) {
        SignalHandler::handle_signal(self, event.signal(), ctx);
    }
}

/// Type alias matching QP/C++'s QActive.
pub type QActive<B> = ActiveObject<B>;

/// Convenient short alias for QActive.
pub type Q<B> = ActiveObject<B>;

