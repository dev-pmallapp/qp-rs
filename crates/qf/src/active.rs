//! Active object abstraction (SRS §3.3).
//!
//! Active objects encapsulate state machines with event queues and execute in
//! priority order under the control of the QP kernel.

#[cfg(feature = "std")]
use std::collections::VecDeque;

#[cfg(not(feature = "std"))]
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

/// Event queue with an occupancy high-water mark.
///
/// The active-object queue is unbounded, so QP/C++'s `QActive::getQueueMin()`
/// (minimum free slots) has no fixed-capacity analog. Instead we track the
/// maximum number of events ever queued simultaneously, which is the meaningful
/// portable metric for sizing a bounded queue on a constrained target.
struct EventQueue {
    buf: VecDeque<DynEvent>,
    high_watermark: usize,
}

impl EventQueue {
    fn new() -> Self {
        Self {
            buf: VecDeque::new(),
            high_watermark: 0,
        }
    }

    /// Records the current length as the new high-water mark if it is larger.
    fn touch_watermark(&mut self) {
        if self.buf.len() > self.high_watermark {
            self.high_watermark = self.buf.len();
        }
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
        queue.buf.pop_front()
    }

    /// Number of events currently waiting in this active object's queue.
    pub fn queue_len(&self) -> usize {
        self.queue.lock().buf.len()
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
        let mut queue = self.queue.lock();
        queue.buf.push_back(event);
        queue.touch_watermark();
    }

    fn post_lifo(&self, event: DynEvent) {
        let mut queue = self.queue.lock();
        queue.buf.push_front(event);
        queue.touch_watermark();
    }

    fn has_events(&self) -> bool {
        !self.queue.lock().buf.is_empty()
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

