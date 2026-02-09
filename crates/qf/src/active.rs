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
    pub fn new(id: ActiveObjectId, trace: Option<TraceHook>) -> Self {
        Self { id, trace }
    }

    pub fn id(&self) -> ActiveObjectId {
        self.id
    }

    pub fn emit_trace(&self, record_type: u8, payload: &[u8]) -> Result<(), TraceError> {
        self.emit_trace_with_timestamp(record_type, payload, true)
    }

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

    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }
}

/// Trait implemented by application state machines.
pub trait ActiveBehavior: Send + 'static {
    fn on_start(&mut self, ctx: &mut ActiveContext);
    fn on_event(&mut self, ctx: &mut ActiveContext, event: DynEvent);
}

/// Object-safe interface used by the kernel.
pub trait ActiveRunnable: Send + Sync {
    fn id(&self) -> ActiveObjectId;
    fn priority(&self) -> u8;
    fn start(&self, trace: Option<TraceHook>);
    fn dispatch_one(&self) -> bool;
    fn post(&self, event: DynEvent);
    fn has_events(&self) -> bool;
}

/// Concrete active object implementation for a specific behavior.
pub struct ActiveObject<B: ActiveBehavior> {
    id: ActiveObjectId,
    priority: u8,
    queue: Mutex<VecDeque<DynEvent>>,
    behavior: Mutex<B>,
    trace_hook: Mutex<Option<TraceHook>>,
}

impl<B: ActiveBehavior> ActiveObject<B> {
    pub fn new(id: ActiveObjectId, priority: u8, behavior: B) -> Arc<Self> {
        Arc::new(Self {
            id,
            priority,
            queue: Mutex::new(VecDeque::new()),
            behavior: Mutex::new(behavior),
            trace_hook: Mutex::new(None),
        })
    }

    fn pop_event(&self) -> Option<DynEvent> {
        let mut queue = self.queue.lock();
        queue.pop_front()
    }

    fn build_context(&self) -> ActiveContext {
        let trace = self.trace_hook.lock().clone();
        ActiveContext::new(self.id, trace)
    }
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
        queue.push_back(event);
    }

    fn has_events(&self) -> bool {
        !self.queue.lock().is_empty()
    }
}

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
    fn on_start(&mut self, _ctx: &mut ActiveContext) {}
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
