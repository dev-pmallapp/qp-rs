//! QXK dual-mode kernel implementation.
//!
//! The QXK kernel combines event-driven active objects (from QF) with
//! extended blocking threads, providing a dual-mode execution model.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

use qf::active::{ActiveObjectId, ActiveObjectRef};
use qf::event::{DynEvent, Signal};
use qf::TraceHook;

use crate::scheduler::{QxkScheduler, ScheduleMode, SchedStatus};
use crate::sync::Arc;
use crate::thread::{ExtendedThread, ThreadConfig, ThreadId};

const MAX_AO_PRIORITY: usize = 63;

/// QXK kernel errors.
#[derive(Debug)]
pub enum QxkKernelError {
    /// Duplicate active object priority.
    DuplicateAoPriority(u8),
    /// Duplicate thread ID.
    DuplicateThreadId(ThreadId),
    /// Active object not found.
    AoNotFound(ActiveObjectId),
    /// Thread not found.
    ThreadNotFound(ThreadId),
    /// Invalid active object priority.
    InvalidAoPriority { priority: u8, reason: &'static str },
    /// Invalid thread priority.
    InvalidThreadPriority { priority: u8, reason: &'static str },
}

impl fmt::Display for QxkKernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateAoPriority(prio) => {
                write!(f, "active object priority {prio} already registered")
            }
            Self::DuplicateThreadId(id) => {
                write!(f, "thread ID {id:?} already registered")
            }
            Self::AoNotFound(id) => write!(f, "active object {id:?} not found"),
            Self::ThreadNotFound(id) => write!(f, "thread {id:?} not found"),
            Self::InvalidAoPriority { priority, reason } => {
                write!(f, "invalid AO priority {priority}: {reason}")
            }
            Self::InvalidThreadPriority { priority, reason } => {
                write!(f, "invalid thread priority {priority}: {reason}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for QxkKernelError {}

#[derive(Clone)]
struct AoRegistration {
    object: ActiveObjectRef,
    priority: u8,
    id: ActiveObjectId,
}

#[derive(Clone)]
struct AoSlot {
    object: ActiveObjectRef,
}

/// Builder for constructing a QXK kernel.
pub struct QxkKernelBuilder {
    ao_registrations: Vec<AoRegistration>,
    thread_configs: Vec<ThreadConfig>,
    trace: Option<TraceHook>,
}

impl QxkKernelBuilder {
    /// Creates a new QXK kernel builder.
    pub fn new() -> Self {
        Self {
            ao_registrations: Vec::new(),
            thread_configs: Vec::new(),
            trace: None,
        }
    }

    /// Registers an active object with the kernel.
    pub fn register_ao(mut self, object: ActiveObjectRef) -> Result<Self, QxkKernelError> {
        let priority = object.priority();
        self.validate_ao_priority(priority)?;
        let id = object.id();
        self.ao_registrations.push(AoRegistration {
            object,
            priority,
            id,
        });
        Ok(self)
    }

    /// Registers an extended thread with the kernel.
    pub fn register_thread(mut self, config: ThreadConfig) -> Result<Self, QxkKernelError> {
        // Check for duplicate thread IDs
        if self.thread_configs.iter().any(|c| c.id == config.id) {
            return Err(QxkKernelError::DuplicateThreadId(config.id));
        }
        self.thread_configs.push(config);
        Ok(self)
    }

    /// Sets the trace hook for kernel events.
    pub fn with_trace_hook(mut self, hook: TraceHook) -> Self {
        self.trace = Some(hook);
        self
    }

    /// Builds the QXK kernel.
    pub fn build(self) -> Result<QxkKernel, QxkKernelError> {
        QxkKernel::new(self.ao_registrations, self.thread_configs, self.trace)
    }

    fn validate_ao_priority(&self, priority: u8) -> Result<(), QxkKernelError> {
        if priority == 0 {
            return Err(QxkKernelError::InvalidAoPriority {
                priority,
                reason: "priority 0 is reserved for idle",
            });
        }
        if priority as usize > MAX_AO_PRIORITY {
            return Err(QxkKernelError::InvalidAoPriority {
                priority,
                reason: "exceeds supported range 1..63",
            });
        }
        Ok(())
    }
}

impl Default for QxkKernelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// QXK dual-mode preemptive kernel.
///
/// Manages both active objects (event-driven) and extended threads (blocking).
pub struct QxkKernel {
    scheduler: Arc<QxkScheduler>,
    ao_slots: Vec<Option<AoSlot>>,
    ao_id_to_prio: BTreeMap<ActiveObjectId, u8>,
    threads: BTreeMap<ThreadId, ExtendedThread>,
    trace: Option<TraceHook>,
}

impl QxkKernel {
    /// Creates a new QXK kernel builder.
    pub fn builder() -> QxkKernelBuilder {
        QxkKernelBuilder::new()
    }

    fn new(
        ao_registrations: Vec<AoRegistration>,
        thread_configs: Vec<ThreadConfig>,
        trace: Option<TraceHook>,
    ) -> Result<Self, QxkKernelError> {
        // Set up active object slots
        let mut ao_slots: Vec<Option<AoSlot>> = vec![None; MAX_AO_PRIORITY + 1];
        let mut ao_id_to_prio = BTreeMap::new();

        for registration in ao_registrations {
            let prio = registration.priority as usize;
            if ao_slots[prio].is_some() {
                return Err(QxkKernelError::DuplicateAoPriority(registration.priority));
            }
            ao_id_to_prio.insert(registration.id, registration.priority);
            ao_slots[prio] = Some(AoSlot {
                object: registration.object,
            });
        }

        // Set up extended threads
        let mut threads = BTreeMap::new();
        for config in thread_configs {
            let id = config.id;
            let thread = ExtendedThread::new(config);
            threads.insert(id, thread);
        }

        let scheduler = Arc::new(QxkScheduler::new(trace.clone()));

        Ok(Self {
            scheduler,
            ao_slots,
            ao_id_to_prio,
            threads,
            trace,
        })
    }

    /// Returns a reference to the scheduler.
    pub fn scheduler(&self) -> Arc<QxkScheduler> {
        Arc::clone(&self.scheduler)
    }

    /// Returns the trace hook.
    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }

    /// Locks the scheduler at the given ceiling.
    pub fn lock_scheduler(&self, ceiling: u8) -> SchedStatus {
        self.scheduler.lock(ceiling)
    }

    /// Unlocks the scheduler.
    pub fn unlock_scheduler(&self, status: SchedStatus) {
        self.scheduler.unlock(status);
        if status.is_locked() {
            // Check if there's work to do after unlocking
            if self.scheduler.has_work() {
                self.dispatch_once();
            }
        }
    }

    /// Starts all active objects and threads.
    pub fn start(&mut self) {
        // Start active objects
        for slot in self.ao_slots.iter().flatten() {
            slot.object.start(self.trace.clone());
            if slot.object.has_events() {
                self.scheduler.mark_ao_ready(slot.object.priority());
            }
        }

        // Mark all threads as ready
        for thread in self.threads.values() {
            if thread.is_ready() {
                self.scheduler
                    .mark_thread_ready(thread.id(), thread.priority());
            }
        }
    }

    /// Posts an event to an active object.
    pub fn post_ao(
        &self,
        target: ActiveObjectId,
        event: DynEvent,
    ) -> Result<(), QxkKernelError> {
        let prio = self
            .ao_id_to_prio
            .get(&target)
            .copied()
            .ok_or(QxkKernelError::AoNotFound(target))?;
        let slot = self.ao_slots[prio as usize]
            .as_ref()
            .expect("kernel registry invariant broken");
        let was_empty = !slot.object.has_events();
        slot.object.post(event);
        if was_empty {
            self.scheduler.mark_ao_ready(prio);
        }
        Ok(())
    }

    /// Posts an event to an active object and runs the kernel until idle.
    pub fn post_ao_and_run(
        &self,
        target: ActiveObjectId,
        event: DynEvent,
    ) -> Result<(), QxkKernelError> {
        self.post_ao(target, event)?;
        self.run_until_idle();
        Ok(())
    }

    /// Publishes an event to all active objects.
    pub fn publish_ao(&self, signal: Signal, event: DynEvent) {
        for (prio, slot_opt) in self.ao_slots.iter().enumerate() {
            if let Some(slot) = slot_opt {
                let was_empty = !slot.object.has_events();
                let mut cloned = event.clone();
                cloned.header.signal = signal;
                slot.object.post(cloned);
                if was_empty {
                    self.scheduler.mark_ao_ready(prio as u8);
                }
            }
        }
    }

    /// Dispatches one unit of work (active object or thread).
    pub fn dispatch_once(&self) -> bool {
        match self.scheduler.plan_next() {
            ScheduleMode::ActiveObject { priority } => {
                self.dispatch_ao(priority);
                true
            }
            ScheduleMode::ExtendedThread { id, .. } => {
                // Note: Thread execution would require cooperative yielding
                // or actual thread support. For now, mark as not ready.
                self.scheduler.mark_thread_not_ready(id);
                true
            }
            ScheduleMode::Idle => false,
        }
    }

    /// Runs the kernel until all work is complete.
    pub fn run_until_idle(&self) {
        while self.dispatch_once() {}
    }

    /// Checks if there is pending work.
    pub fn has_pending_work(&self) -> bool {
        self.scheduler.has_work()
    }

    fn dispatch_ao(&self, priority: u8) {
        if let Some(slot) = &self.ao_slots[priority as usize] {
            let processed = slot.object.dispatch_one();
            debug_assert!(processed, "scheduled AO had no event");

            if !slot.object.has_events() {
                self.scheduler.mark_ao_not_ready(priority);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qf::active::{new_active_object, ActiveContext, SignalHandler};
    use qf::event::Signal;
    use std::sync::{Arc as StdArc, Mutex};

    #[derive(Clone)]
    struct Recorder {
        id: ActiveObjectId,
        log: StdArc<Mutex<Vec<(ActiveObjectId, Signal)>>>,
    }

    impl Recorder {
        fn new(id: ActiveObjectId, log: StdArc<Mutex<Vec<(ActiveObjectId, Signal)>>>) -> Self {
            Self { id, log }
        }
    }

    impl SignalHandler for Recorder {
        fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActiveContext) {
            self.log.lock().unwrap().push((self.id, signal));
        }
    }

    #[test]
    fn kernel_builds_with_aos() -> Result<(), QxkKernelError> {
        let log = StdArc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(1);
        let ao = new_active_object(ao_id, 5, Recorder::new(ao_id, Arc::clone(&log)));

        let _kernel = QxkKernel::builder().register_ao(ao)?.build()?;
        Ok(())
    }

    #[test]
    fn kernel_dispatches_ao_events() -> Result<(), QxkKernelError> {
        let log = StdArc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(2);
        let ao = new_active_object(ao_id, 3, Recorder::new(ao_id, Arc::clone(&log)));

        let mut kernel = QxkKernel::builder().register_ao(ao)?.build()?;
        kernel.start();

        kernel.post_ao_and_run(ao_id, DynEvent::empty_dyn(Signal(42)))?;

        let entries = log.lock().unwrap();
        assert_eq!(entries.as_slice(), &[(ao_id, Signal(42))]);
        Ok(())
    }

    #[test]
    fn publish_delivers_to_all_aos() -> Result<(), QxkKernelError> {
        let log = StdArc::new(Mutex::new(Vec::new()));
        let ao1_id = ActiveObjectId::new(3);
        let ao2_id = ActiveObjectId::new(4);

        let ao1 = new_active_object(ao1_id, 2, Recorder::new(ao1_id, Arc::clone(&log)));
        let ao2 = new_active_object(ao2_id, 5, Recorder::new(ao2_id, Arc::clone(&log)));

        let mut kernel = QxkKernel::builder()
            .register_ao(ao1)?
            .register_ao(ao2)?
            .build()?;
        kernel.start();

        kernel.publish_ao(Signal(99), DynEvent::empty_dyn(Signal(0)));
        kernel.run_until_idle();

        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 2);
        // Higher priority dispatched first
        assert_eq!(entries[0], (ao2_id, Signal(99)));
        assert_eq!(entries[1], (ao1_id, Signal(99)));
        Ok(())
    }
}
