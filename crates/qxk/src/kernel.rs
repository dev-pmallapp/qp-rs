//! QXK dual-mode kernel implementation.
//!
//! The QXK kernel combines event-driven active objects (from QF) with
//! extended blocking threads, providing a dual-mode execution model.

#[cfg(not(feature = "static-alloc"))]
use alloc::collections::BTreeMap;
use core::fmt;

use qf::active::{ActiveObjectId, ActiveObjectRef};
use qf::event::{DynEvent, Signal};
use qf::TraceHook;

use crate::scheduler::{QxkScheduler, ScheduleMode, SchedStatus};
#[cfg(not(feature = "static-alloc"))]
use crate::sync::Arc;
use crate::sync::Mutex;
use crate::thread::{ExtendedThread, ThreadAction, ThreadConfig, ThreadId};

const MAX_AO_PRIORITY: usize = 63;

/// Active-object registration list. Dynamic: heap [`Vec`]; `static-alloc`:
/// heap-free [`heapless::Vec`] bounded by the priority range.
#[cfg(not(feature = "static-alloc"))]
type AoRegVec = alloc::vec::Vec<AoRegistration>;
#[cfg(feature = "static-alloc")]
type AoRegVec = heapless::Vec<AoRegistration, { MAX_AO_PRIORITY + 1 }>;

/// Thread-config registration list. Dynamic: heap [`Vec`]; `static-alloc`:
/// heap-free [`heapless::Vec`] bounded by [`crate::MAX_THREADS`].
#[cfg(not(feature = "static-alloc"))]
type ThreadCfgVec = alloc::vec::Vec<ThreadConfig>;
#[cfg(feature = "static-alloc")]
type ThreadCfgVec = heapless::Vec<ThreadConfig, { crate::MAX_THREADS }>;

/// Live extended-thread storage. Dynamic: `BTreeMap` of `Arc<Mutex<_>>` (lookup
/// by id); `static-alloc`: heap-free `heapless::Vec` of `(id, Mutex<_>)` scanned
/// by id (threads are kernel-private, so no shared handle is needed).
#[cfg(not(feature = "static-alloc"))]
type ThreadStore = BTreeMap<ThreadId, Arc<Mutex<ExtendedThread>>>;
#[cfg(feature = "static-alloc")]
type ThreadStore = heapless::Vec<(ThreadId, Mutex<ExtendedThread>), { crate::MAX_THREADS }>;

/// Shared handle to the scheduler. Dynamic: `Arc<QxkScheduler>`; `static-alloc`:
/// the scheduler is owned inline and handed out by reference.
#[cfg(not(feature = "static-alloc"))]
type SchedOwned = Arc<QxkScheduler>;
#[cfg(feature = "static-alloc")]
type SchedOwned = QxkScheduler;

/// Push an AO registration, faulting (crash-only) if the heap-free vector is full.
#[inline]
fn push_ao_reg(v: &mut AoRegVec, r: AoRegistration) {
    #[cfg(not(feature = "static-alloc"))]
    v.push(r);
    #[cfg(feature = "static-alloc")]
    if v.push(r).is_err() {
        qf::fusa::on_error(module_path!(), line!());
    }
}

/// Push a thread config, faulting (crash-only) if the heap-free vector is full.
#[inline]
fn push_thread_cfg(v: &mut ThreadCfgVec, c: ThreadConfig) {
    #[cfg(not(feature = "static-alloc"))]
    v.push(c);
    #[cfg(feature = "static-alloc")]
    if v.push(c).is_err() {
        qf::fusa::on_error(module_path!(), line!());
    }
}

/// Insert a live thread into the store, faulting (crash-only) if the heap-free
/// vector is full.
#[inline]
fn insert_thread(s: &mut ThreadStore, id: ThreadId, t: ExtendedThread) {
    #[cfg(not(feature = "static-alloc"))]
    {
        s.insert(id, Arc::new(Mutex::new(t)));
    }
    #[cfg(feature = "static-alloc")]
    if s.push((id, Mutex::new(t))).is_err() {
        qf::fusa::on_error(module_path!(), line!());
    }
}

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
    InvalidAoPriority {
        /// The offending priority value.
        priority: u8,
        /// Why the priority was rejected.
        reason: &'static str,
    },
    /// Invalid thread priority.
    InvalidThreadPriority {
        /// The offending priority value.
        priority: u8,
        /// Why the priority was rejected.
        reason: &'static str,
    },
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
    // Used to build the id→priority map on the dynamic build; the heap-free
    // build scans `ao_slots` by id instead, so the field is set but not read there.
    #[cfg_attr(feature = "static-alloc", allow(dead_code))]
    id: ActiveObjectId,
}

#[derive(Clone)]
struct AoSlot {
    object: ActiveObjectRef,
}

/// Builder for constructing a QXK kernel.
pub struct QxkKernelBuilder {
    ao_registrations: AoRegVec,
    thread_configs: ThreadCfgVec,
    trace: Option<TraceHook>,
}

impl QxkKernelBuilder {
    /// Creates a new QXK kernel builder.
    pub fn new() -> Self {
        Self {
            ao_registrations: AoRegVec::new(),
            thread_configs: ThreadCfgVec::new(),
            trace: None,
        }
    }

    /// Registers an active object with the kernel.
    pub fn register_ao(mut self, object: ActiveObjectRef) -> Result<Self, QxkKernelError> {
        let priority = object.priority();
        self.validate_ao_priority(priority)?;
        let id = object.id();
        push_ao_reg(&mut self.ao_registrations, AoRegistration {
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
        push_thread_cfg(&mut self.thread_configs, config);
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
    scheduler: SchedOwned,
    /// Priority-indexed AO registry (idle slot 0 unused). A fixed array, so it
    /// needs no heap on either build.
    ao_slots: [Option<AoSlot>; MAX_AO_PRIORITY + 1],
    /// Id → priority map for `post_ao`. Heap-free builds drop it and scan `ao_slots`.
    #[cfg(not(feature = "static-alloc"))]
    ao_id_to_prio: BTreeMap<ActiveObjectId, u8>,
    threads: ThreadStore,
    trace: Option<TraceHook>,
}

impl QxkKernel {
    /// Creates a new QXK kernel builder.
    pub fn builder() -> QxkKernelBuilder {
        QxkKernelBuilder::new()
    }

    fn new(
        ao_registrations: AoRegVec,
        thread_configs: ThreadCfgVec,
        trace: Option<TraceHook>,
    ) -> Result<Self, QxkKernelError> {
        // Set up active object slots
        let mut ao_slots: [Option<AoSlot>; MAX_AO_PRIORITY + 1] =
            core::array::from_fn(|_| None);
        #[cfg(not(feature = "static-alloc"))]
        let mut ao_id_to_prio = BTreeMap::new();

        for registration in ao_registrations {
            let prio = registration.priority as usize;
            if ao_slots[prio].is_some() {
                return Err(QxkKernelError::DuplicateAoPriority(registration.priority));
            }
            #[cfg(not(feature = "static-alloc"))]
            ao_id_to_prio.insert(registration.id, registration.priority);
            ao_slots[prio] = Some(AoSlot {
                object: registration.object,
            });
        }

        // Set up extended threads
        let mut threads = ThreadStore::new();
        for config in thread_configs {
            let id = config.id;
            let thread = ExtendedThread::new(config);
            insert_thread(&mut threads, id, thread);
        }

        let scheduler = Self::new_scheduler(trace.clone());

        Ok(Self {
            scheduler,
            ao_slots,
            #[cfg(not(feature = "static-alloc"))]
            ao_id_to_prio,
            threads,
            trace,
        })
    }

    #[cfg(not(feature = "static-alloc"))]
    #[inline]
    fn new_scheduler(trace: Option<TraceHook>) -> SchedOwned {
        Arc::new(QxkScheduler::new(trace))
    }
    #[cfg(feature = "static-alloc")]
    #[inline]
    fn new_scheduler(trace: Option<TraceHook>) -> SchedOwned {
        QxkScheduler::new(trace)
    }

    /// Borrows the scheduler. Works uniformly across builds: the dynamic build's
    /// `Arc` deref-coerces to `&QxkScheduler`, the heap-free build owns it inline.
    #[inline]
    fn sched(&self) -> &QxkScheduler {
        &self.scheduler
    }

    /// Returns a handle to the underlying scheduler: a shared `Arc` on the
    /// dynamic build, a borrow under `static-alloc` (the kernel owns it inline).
    #[cfg(not(feature = "static-alloc"))]
    pub fn scheduler(&self) -> Arc<QxkScheduler> {
        Arc::clone(&self.scheduler)
    }
    /// See the dynamic variant above.
    #[cfg(feature = "static-alloc")]
    pub fn scheduler(&self) -> &QxkScheduler {
        &self.scheduler
    }

    /// Iterates the live thread mutexes (build-agnostic view over the store).
    #[cfg(not(feature = "static-alloc"))]
    fn thread_mutexes(&self) -> impl Iterator<Item = &Mutex<ExtendedThread>> {
        self.threads.values().map(|a| a.as_ref())
    }
    /// See the dynamic variant above.
    #[cfg(feature = "static-alloc")]
    fn thread_mutexes(&self) -> impl Iterator<Item = &Mutex<ExtendedThread>> {
        self.threads.iter().map(|(_, m)| m)
    }

    /// Looks up a live thread mutex by id (map lookup / heap-free scan).
    #[cfg(not(feature = "static-alloc"))]
    fn find_thread(&self, id: ThreadId) -> Option<&Mutex<ExtendedThread>> {
        self.threads.get(&id).map(|a| a.as_ref())
    }
    /// See the dynamic variant above.
    #[cfg(feature = "static-alloc")]
    fn find_thread(&self, id: ThreadId) -> Option<&Mutex<ExtendedThread>> {
        self.threads
            .iter()
            .find(|(tid, _)| *tid == id)
            .map(|(_, m)| m)
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
        for thread_mtx in self.thread_mutexes() {
            let thread = thread_mtx.lock();
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
        // Dynamic: O(log n) id→priority map. Heap-free: scan the fixed slot array.
        #[cfg(not(feature = "static-alloc"))]
        let prio = self
            .ao_id_to_prio
            .get(&target)
            .copied()
            .ok_or(QxkKernelError::AoNotFound(target))?;
        #[cfg(feature = "static-alloc")]
        let prio = self
            .ao_slots
            .iter()
            .flatten()
            .find(|slot| slot.object.id() == target)
            .map(|slot| slot.object.priority())
            .ok_or(QxkKernelError::AoNotFound(target))?;
        let slot = self.ao_slots[prio as usize]
            .as_ref()
            .unwrap_or_else(|| qf::fusa::on_error(module_path!(), line!()));
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
        let mode = self.scheduler.plan_next();
        self.scheduler.set_active(mode);
        match mode {
            ScheduleMode::ActiveObject { priority } => {
                self.dispatch_ao(priority);
                self.scheduler.complete_execution(mode);
                true
            }
            ScheduleMode::ExtendedThread { id, priority } => {
                if let Some(thread_mtx) = self.find_thread(id) {
                    let mut thread = thread_mtx.lock();
                    let action = thread.poll(self.sched());

                    self.scheduler.complete_execution(mode);

                    match action {
                        ThreadAction::Continue => {
                            // Keep thread ready, will be polled again
                            true
                        }
                        ThreadAction::Yield => {
                            // Thread yielded, stays ready
                            self.scheduler.mark_thread_ready(id, priority);
                            true
                        }
                        ThreadAction::Blocked => {
                            // Primitive already blocked thread via scheduler
                            true
                        }
                        ThreadAction::Terminated => {
                            // Thread finished
                            self.scheduler.mark_thread_not_ready(id);
                            true
                        }
                    }
                } else {
                    self.scheduler.complete_execution(mode);
                    self.scheduler.mark_thread_not_ready(id);
                    false
                }
            }
            ScheduleMode::Idle => {
                self.scheduler.complete_execution(mode);
                false
            }
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
        let ao = new_active_object(ao_id, 5, Recorder::new(ao_id, StdArc::clone(&log)));

        let _kernel = QxkKernel::builder().register_ao(ao)?.build()?;
        Ok(())
    }

    #[test]
    fn kernel_dispatches_ao_events() -> Result<(), QxkKernelError> {
        let log = StdArc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(2);
        let ao = new_active_object(ao_id, 3, Recorder::new(ao_id, StdArc::clone(&log)));

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

        let ao1 = new_active_object(ao1_id, 2, Recorder::new(ao1_id, StdArc::clone(&log)));
        let ao2 = new_active_object(ao2_id, 5, Recorder::new(ao2_id, StdArc::clone(&log)));

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

    #[test]
    #[cfg(feature = "smp")]
    fn test_smp_active_object_isolation_and_rtc() -> Result<(), QxkKernelError> {
        use std::sync::{Arc, Mutex};
        use qf::active::ActiveContext;

        #[derive(Clone)]
        struct MockBehavior {
            id: ActiveObjectId,
            active_threads: Arc<Mutex<usize>>,
            max_concurrent_threads: Arc<Mutex<usize>>,
        }

        impl SignalHandler for MockBehavior {
            fn handle_signal(&mut self, _signal: Signal, _ctx: &mut ActiveContext) {
                {
                    let mut active = self.active_threads.lock().unwrap();
                    *active += 1;
                    let mut max = self.max_concurrent_threads.lock().unwrap();
                    if *active > *max {
                        *max = *active;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                {
                    let mut active = self.active_threads.lock().unwrap();
                    *active -= 1;
                }
            }
        }

        let active_threads = Arc::new(Mutex::new(0));
        let max_concurrent_threads = Arc::new(Mutex::new(0));

        let ao_id = ActiveObjectId::new(10);
        let ao = new_active_object(
            ao_id,
            10,
            MockBehavior {
                id: ao_id,
                active_threads: Arc::clone(&active_threads),
                max_concurrent_threads: Arc::clone(&max_concurrent_threads),
            },
        );

        let mut kernel = QxkKernel::builder()
            .register_ao(ao)?
            .build()
            .expect("kernel build succeeded");

        kernel.start();

        for i in 0..30 {
            kernel.post_ao(ao_id, DynEvent::empty_dyn(Signal(i))).unwrap();
        }

        let kernel_arc = Arc::new(kernel);
        let mut handles = Vec::new();
        for _ in 0..4 {
            let kernel_clone = Arc::clone(&kernel_arc);
            handles.push(std::thread::spawn(move || {
                while kernel_clone.has_pending_work() {
                    kernel_clone.dispatch_once();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let max_concurrent = *max_concurrent_threads.lock().unwrap();
        assert_eq!(max_concurrent, 1, "AO behavior was executed concurrently by multiple cores!");
        Ok(())
    }

    #[test]
    #[cfg(feature = "smp")]
    fn test_smp_thread_isolation() -> Result<(), QxkKernelError> {
        use std::sync::{Arc, Mutex};
        use crate::thread::ThreadAction;

        let active_threads = Arc::new(Mutex::new(0));
        let max_concurrent_threads = Arc::new(Mutex::new(0));

        let active_threads_clone = Arc::clone(&active_threads);
        let max_concurrent_threads_clone = Arc::clone(&max_concurrent_threads);

        let thread_config = ThreadConfig::new(
            ThreadId(1),
            crate::thread::ThreadPriority(5),
            crate::thread::thread_handler(move |ctx| {
                {
                    let mut active = active_threads_clone.lock().unwrap();
                    *active += 1;
                    let mut max = max_concurrent_threads_clone.lock().unwrap();
                    if *active > *max {
                        *max = *active;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                {
                    let mut active = active_threads_clone.lock().unwrap();
                    *active -= 1;
                }
                if ctx.iteration() < 10 {
                    ThreadAction::Continue
                } else {
                    ThreadAction::Terminated
                }
            }),
        );

        let mut kernel = QxkKernel::builder()
            .register_thread(thread_config)?
            .build()
            .expect("kernel build succeeded");

        kernel.start();

        let kernel_arc = Arc::new(kernel);
        let mut handles = Vec::new();
        for _ in 0..4 {
            let kernel_clone = Arc::clone(&kernel_arc);
            handles.push(std::thread::spawn(move || {
                while kernel_clone.has_pending_work() {
                    kernel_clone.dispatch_once();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let max_concurrent = *max_concurrent_threads.lock().unwrap();
        assert_eq!(max_concurrent, 1, "Thread was executed/polled concurrently by multiple cores!");
        Ok(())
    }
}
