//! Cooperative kernel and scheduling services (SRS §3.4).

use core::fmt;
use portable_atomic::{AtomicBool, Ordering};

#[cfg(not(feature = "static-alloc"))]
use alloc::collections::BTreeMap;
#[cfg(not(feature = "static-alloc"))]
use alloc::vec::Vec;

#[cfg(not(feature = "static-alloc"))]
use crate::sync::Arc;
// `Mutex` guards the cooperative scheduler state; the SMP kernel uses lock-free
// atomics instead, so it is only needed off the `smp` path.
#[cfg(not(feature = "smp"))]
use crate::sync::Mutex;
use crate::trace::{TraceError, TraceHook};

use crate::active::{ActiveObjectId, ActiveObjectRef};
use crate::event::{DynEvent, Signal};
use crate::pubsub::PubSubTable;

const QS_SCHED_LOCK: u8 = 50;
const QS_SCHED_UNLOCK: u8 = 51;
const QS_SCHED_NEXT: u8 = 52;
// The SMP scheduler does not emit a per-core idle record from this path.
#[cfg(not(feature = "smp"))]
const QS_SCHED_IDLE: u8 = 53;

/// Maximum active objects in the heap-free registry (idle priority 0 plus the
/// 1..=63 application range that the preemptive kernels permit).
#[cfg(feature = "static-alloc")]
pub const MAX_ACTIVE: usize = 64;

/// The registry vector for active-object handles. Dynamic: a heap [`Vec`];
/// `static-alloc`: a fixed-capacity, heap-free [`heapless::Vec`].
#[cfg(not(feature = "static-alloc"))]
type ObjVec = Vec<ActiveObjectRef>;
#[cfg(feature = "static-alloc")]
type ObjVec = heapless::Vec<ActiveObjectRef, MAX_ACTIVE>;

/// Push an active-object handle into a registry vector, faulting (crash-only)
/// if the fixed-capacity heap-free vector is full.
#[inline]
fn push_obj(v: &mut ObjVec, o: ActiveObjectRef) {
    #[cfg(not(feature = "static-alloc"))]
    v.push(o);
    #[cfg(feature = "static-alloc")]
    if v.push(o).is_err() {
        crate::fusa::on_error(module_path!(), line!());
    }
}

/// Clone a registry handle: an `Arc` refcount bump on the dynamic build, a
/// trivial pointer copy under `static-alloc`.
#[cfg(feature = "smp")]
#[inline]
fn clone_ref(a: &ActiveObjectRef) -> ActiveObjectRef {
    #[cfg(not(feature = "static-alloc"))]
    {
        Arc::clone(a)
    }
    #[cfg(feature = "static-alloc")]
    {
        *a
    }
}

// The SMP kernel keeps its scheduler state in lock-free atomics, not this
// `Mutex`-guarded struct.
#[cfg(not(feature = "smp"))]
#[derive(Default)]
struct SchedulerState {
    prev_prio: u8,
    sched_ceiling: u8,
}

/// Configuration for the QF kernel.
///
/// Provides system sizing metadata required by QS tracing and runtime
/// configuration options like idle callbacks.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Application name reported in the QS `TARGET_INFO` record.
    pub name: &'static str,
    /// Maximum number of active objects the system is sized for.
    pub max_active: u8,
    /// Maximum number of event pools.
    pub max_event_pools: u8,
    /// Maximum number of tick-rate domains.
    pub max_tick_rate: u8,
    /// Byte width of event-queue counters (for QS encoding).
    pub event_queue_ctr_size: u8,
    /// Byte width of time-event counters (for QS encoding).
    pub time_event_ctr_size: u8,
    /// Optional callback invoked when the kernel goes idle.
    pub idle_callback: Option<fn()>,
    /// Framework version reported to QS (e.g. `740`).
    pub version: u16,
    /// Optional free-form build information string for QS.
    pub build_info: Option<&'static str>,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            name: "QP",
            max_active: 16,
            max_event_pools: 3,
            max_tick_rate: 4,
            event_queue_ctr_size: 2,
            time_event_ctr_size: 2,
            idle_callback: None,
            version: 740,
            build_info: None,
        }
    }
}

impl KernelConfig {
    /// Creates a new kernel configuration builder.
    pub fn builder() -> KernelConfigBuilder {
        KernelConfigBuilder::default()
    }

    /// Converts this configuration to a QS TargetInfo record.
    #[cfg(feature = "qs")]
    pub fn to_target_info(&self) -> qs::predefined::TargetInfo {
        use qs::predefined::TargetInfo;

        TargetInfo {
            is_reset: 0xFF,
            version: self.version,
            signal_size: 2,
            event_size: 2,
            equeue_ctr_size: self.event_queue_ctr_size,
            time_evt_ctr_size: self.time_event_ctr_size,
            mpool_size_size: 2,
            mpool_ctr_size: 2,
            obj_ptr_size: core::mem::size_of::<usize>() as u8,
            fun_ptr_size: core::mem::size_of::<usize>() as u8,
            time_size: 4,
            max_active: self.max_active,
            max_event_pools: self.max_event_pools,
            max_tick_rate: self.max_tick_rate,
            build_time: (0, 0, 0),
            build_date: (1, 1, 26),
        }
    }
}

/// Builder for ergonomic kernel configuration construction.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct KernelConfigBuilder {
    config: KernelConfig,
}


impl KernelConfigBuilder {
    /// Sets the kernel name.
    pub fn name(mut self, name: &'static str) -> Self {
        self.config.name = name;
        self
    }

    /// Sets the maximum number of active objects.
    pub fn max_active(mut self, max: u8) -> Self {
        self.config.max_active = max;
        self
    }

    /// Sets the maximum number of event pools.
    pub fn max_event_pools(mut self, max: u8) -> Self {
        self.config.max_event_pools = max;
        self
    }

    /// Sets the maximum tick rate.
    pub fn max_tick_rate(mut self, max: u8) -> Self {
        self.config.max_tick_rate = max;
        self
    }

    /// Sets the counter sizes for event queues and time events.
    ///
    /// # Parameters
    /// - `queue`: Size in bytes for event queue counters (1, 2, or 4)
    /// - `time`: Size in bytes for time event counters (1, 2, or 4)
    pub fn counter_sizes(mut self, queue: u8, time: u8) -> Self {
        self.config.event_queue_ctr_size = queue;
        self.config.time_event_ctr_size = time;
        self
    }

    /// Sets the idle callback function.
    pub fn idle_callback(mut self, callback: fn()) -> Self {
        self.config.idle_callback = Some(callback);
        self
    }

    /// Sets the version number.
    pub fn version(mut self, version: u16) -> Self {
        self.config.version = version;
        self
    }

    /// Sets build information string.
    pub fn build_info(mut self, info: &'static str) -> Self {
        self.config.build_info = Some(info);
        self
    }

    /// Builds the kernel configuration.
    pub fn build(self) -> KernelConfig {
        self.config
    }
}

/// Builder for the cooperative [`Kernel`]: register active objects, attach a
/// trace hook, then [`build`](Self::build).
pub struct KernelBuilder {
    config: KernelConfig,
    objects: ObjVec,
    trace: Option<TraceHook>,
    pubsub: Option<PubSubTable>,
}

impl KernelBuilder {
    /// Creates a builder seeded with the given configuration.
    pub fn new(config: KernelConfig) -> Self {
        Self {
            config,
            objects: ObjVec::new(),
            trace: None,
            pubsub: None,
        }
    }

    /// Initializes publish-subscribe with subscriber bitmap table up to `max_signal`.
    pub fn ps_init(mut self, max_signal: u16) -> Self {
        self.pubsub = Some(PubSubTable::new(max_signal));
        self
    }

    /// Registers an active object with the kernel.
    pub fn register(mut self, object: ActiveObjectRef) -> Self {
        push_obj(&mut self.objects, object);
        self
    }

    /// Attaches a QS trace hook to the kernel.
    pub fn with_trace_hook(mut self, hook: TraceHook) -> Self {
        self.trace = Some(hook);
        self
    }

    /// Sorts the registered objects by priority and constructs the [`QvKernel`].
    pub fn build(mut self) -> QvKernel {
        // `sort_unstable_by_key` is in `core` (no alloc) and is fine here:
        // active-object priorities are unique, so stability is irrelevant.
        self.objects.sort_unstable_by_key(|ao| ao.priority());
        QvKernel::new(self.config, self.objects, self.trace, self.pubsub)
    }
}

/// Errors returned by cooperative-kernel operations.
#[derive(Debug)]
pub enum KernelError {
    /// No active object is registered for the given id.
    NotFound(ActiveObjectId),
    /// Emitting a QS trace record failed.
    Trace(TraceError),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "active object {id:?} not found"),
            Self::Trace(_) => write!(f, "trace error"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for KernelError {}

impl From<TraceError> for KernelError {
    fn from(value: TraceError) -> Self {
        Self::Trace(value)
    }
}

#[cfg(feature = "smp")]
const CORE_ID_NONE: u8 = 0xFF;

#[cfg(feature = "smp")]
pub struct MpsActiveSlot {
    pub object: ActiveObjectRef,
    pub executing_core: portable_atomic::AtomicU8,
}

/// SMP registry vector. Dynamic: heap [`Vec`]; `static-alloc`: heap-free
/// [`heapless::Vec`].
#[cfg(all(feature = "smp", not(feature = "static-alloc")))]
type SlotVec = Vec<MpsActiveSlot>;
#[cfg(all(feature = "smp", feature = "static-alloc"))]
type SlotVec = heapless::Vec<MpsActiveSlot, MAX_ACTIVE>;


/// Cooperative, priority-based QF kernel: dispatches registered active objects
/// run-to-completion, highest priority first, with scheduler-ceiling locking.
///
/// This is the QP/C++ **QV** kernel equivalent (cooperative, non-preemptive,
/// single-stack). [`Kernel`] is kept as a backwards-compatible alias.
pub struct QvKernel {
    config: KernelConfig,
    #[cfg(not(feature = "smp"))]
    objects: ObjVec,
    #[cfg(feature = "smp")]
    slots: SlotVec,
    /// Id → handle index for O(log n) `post`. Heap-free builds drop this map and
    /// scan the registry instead (small, fixed AO counts).
    #[cfg(not(feature = "static-alloc"))]
    by_id: BTreeMap<ActiveObjectId, ActiveObjectRef>,
    trace: Option<TraceHook>,
    #[cfg(not(feature = "smp"))]
    scheduler: Mutex<SchedulerState>,
    #[cfg(feature = "smp")]
    sched_ceiling: portable_atomic::AtomicU8,
    /// Set by `stop()` to break out of a `run()` loop.
    stop_flag: AtomicBool,
    pubsub: Option<PubSubTable>,
}

/// Backwards-compatible alias for [`QvKernel`], the QP/C++ **QV**-equivalent
/// cooperative kernel. Prefer `QvKernel` in new code.
pub type Kernel = QvKernel;

impl QvKernel {
    /// Returns a builder using the default [`KernelConfig`].
    pub fn builder() -> KernelBuilder {
        KernelBuilder::new(KernelConfig::default())
    }

    /// Returns a builder seeded with the given [`KernelConfig`].
    pub fn with_config(config: KernelConfig) -> KernelBuilder {
        KernelBuilder::new(config)
    }

    /// Posts an event to the target active object's queue.
    pub fn post(&self, target: ActiveObjectId, event: DynEvent) -> Result<(), KernelError> {
        // Dynamic build: O(log n) id→handle map. Heap-free build: linear scan of
        // the (small, fixed) registry — no `BTreeMap` allocation.
        #[cfg(not(feature = "static-alloc"))]
        if let Some(ao) = self.by_id.get(&target) {
            ao.post(event);
            return Ok(());
        }
        #[cfg(feature = "static-alloc")]
        {
            #[cfg(not(feature = "smp"))]
            let iter = self.objects.iter();
            #[cfg(feature = "smp")]
            let iter = self.slots.iter().map(|s| &s.object);
            for ao in iter {
                if ao.id() == target {
                    ao.post(event);
                    return Ok(());
                }
            }
        }
        Err(KernelError::NotFound(target))
    }

    /// Broadcasts or multicasts an event (with `signal`) to registered active objects.
    pub fn publish(&self, signal: Signal, event: DynEvent) {
        if let Some(ref pubsub) = self.pubsub {
            self.emit_publish(signal);
            let subscribers = pubsub.subscribers(signal);
            #[cfg(not(feature = "smp"))]
            let iter = self.objects.iter();
            #[cfg(feature = "smp")]
            let iter = self.slots.iter().map(|s| &s.object);

            for ao in iter {
                let priority = ao.priority();
                if (subscribers & (1u64 << priority)) != 0 {
                    let mut cloned = event.clone();
                    cloned.header.signal = signal;
                    ao.post(cloned);
                }
            }
        } else {
            #[cfg(not(feature = "smp"))]
            let iter = self.objects.iter();
            #[cfg(feature = "smp")]
            let iter = self.slots.iter().map(|s| &s.object);

            for ao in iter {
                // Basic publish duplicates the event header, but payload is shared via Arc.
                let mut cloned = event.clone();
                cloned.header.signal = signal;
                ao.post(cloned);
            }
        }
    }

    /// Starts every registered active object, installing the trace hook.
    pub fn start(&self) {
        #[cfg(not(feature = "smp"))]
        let iter = self.objects.iter();
        #[cfg(feature = "smp")]
        let iter = self.slots.iter().map(|s| &s.object);

        for ao in iter {
            ao.start(self.trace.clone());
        }
    }

    /// Dispatches ready active objects until none remain, then runs the
    /// configured idle callback (if any).
    pub fn run_until_idle(&self) {
        while self.dispatch_once() {}
        // Call idle callback if configured
        if let Some(idle_cb) = self.config.idle_callback {
            idle_cb();
        }
    }

    /// Blocking run loop equivalent to `QF::run()` in QP/C++.
    ///
    /// Calls `tick_fn` once per iteration (for advancing time events), then
    /// drains all pending events. Returns when `stop()` is called.
    ///
    /// `start()` is called automatically before the first iteration.
    pub fn run(&self, mut tick_fn: impl FnMut()) {
        self.start();
        self.stop_flag.store(false, Ordering::Release);
        loop {
            if self.stop_flag.load(Ordering::Acquire) {
                break;
            }
            tick_fn();
            self.run_until_idle();
        }
    }

    /// Signal the `run()` loop to exit.
    ///
    /// Thread-safe; may be called from any context including a signal handler.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Release);
    }

    /// `true` if any registered AO has queued events.
    pub fn has_pending_work(&self) -> bool {
        #[cfg(not(feature = "smp"))]
        let mut iter = self.objects.iter();
        #[cfg(feature = "smp")]
        let mut iter = self.slots.iter().map(|s| &s.object);

        iter.any(|ao| ao.has_events())
    }

    /// Post `event` to `target` from an ISR context.
    ///
    /// Semantically equivalent to `QActive::postFromISR_()` in QP/C++.
    /// The caller must have entered ISR context with `qk_isr_entry!()`.
    pub fn post_from_isr(&self, target: ActiveObjectId, event: DynEvent) -> Result<(), KernelError> {
        debug_assert!(
            crate::isr::in_isr(),
            "post_from_isr called outside ISR context"
        );
        self.post(target, event)
    }

    /// Publish `event` to all registered AOs from an ISR context.
    ///
    /// Semantically equivalent to `QActive::publishFromISR_()` in QP/C++.
    /// The caller must have entered ISR context with `qk_isr_entry!()`.
    pub fn publish_from_isr(&self, signal: Signal, event: DynEvent) {
        debug_assert!(
            crate::isr::in_isr(),
            "publish_from_isr called outside ISR context"
        );
        self.publish(signal, event)
    }

    /// Returns the kernel configuration.
    pub fn config(&self) -> &KernelConfig {
        &self.config
    }

    /// Dispatches one event to the highest-priority ready active object that the
    /// scheduler ceiling permits; returns `true` if an event was handled.
    #[cfg(not(feature = "smp"))]
    pub fn dispatch_once(&self) -> bool {
        let candidate = self
            .objects
            .iter()
            .rev()
            .find(|ao| ao.has_events())
            .cloned();

        if let Some(ao) = candidate {
            // (record, payload buffer, payload length) — a fixed stack buffer so
            // the trace record carries no heap allocation.
            let mut note: Option<(u8, [u8; 2], usize)> = None;
            let mut should_dispatch = true;

            {
                let mut scheduler = self.scheduler.lock();
                let prio = ao.priority();
                if prio <= scheduler.sched_ceiling {
                    should_dispatch = false;
                    if scheduler.prev_prio != 0 {
                        let prev = scheduler.prev_prio;
                        scheduler.prev_prio = 0;
                        note = Some((QS_SCHED_IDLE, [prev, 0], 1));
                    }
                } else {
                    let prev = scheduler.prev_prio;
                    if prio != prev {
                        note = Some((QS_SCHED_NEXT, [prio, prev], 2));
                    }
                    scheduler.prev_prio = prio;
                }
            }

            if let Some((record, buf, len)) = note {
                self.emit_scheduler_record(record, &buf[..len]);
            }

            if !should_dispatch {
                return false;
            }

            ao.dispatch_one()
        } else {
            let mut note: Option<(u8, [u8; 2], usize)> = None;
            {
                let mut scheduler = self.scheduler.lock();
                if scheduler.prev_prio != 0 {
                    let prev = scheduler.prev_prio;
                    scheduler.prev_prio = 0;
                    note = Some((QS_SCHED_IDLE, [prev, 0], 1));
                }
            }

            if let Some((record, buf, len)) = note {
                self.emit_scheduler_record(record, &buf[..len]);
            }

            false
        }
    }

    /// Dispatches one event to the highest-priority ready active object that the
    /// scheduler ceiling permits; returns `true` if an event was handled.
    #[cfg(feature = "smp")]
    pub fn dispatch_once(&self) -> bool {
        let core_id = crate::port::current_core_id();
        let ceiling = self.sched_ceiling.load(Ordering::Acquire);

        // Find the highest priority ready and unclaimed active object
        let candidate_slot = self
            .slots
            .iter()
            .rev()
            .find(|slot| {
                let prio = slot.object.priority();
                prio > ceiling
                    && slot.object.has_events()
                    && slot.executing_core.load(Ordering::Relaxed) == CORE_ID_NONE
            });

        if let Some(slot) = candidate_slot {
            // Try to claim the active object atomically
            if slot.executing_core.compare_exchange(
                CORE_ID_NONE,
                core_id,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                let mut dispatched = false;
                if slot.object.has_events() {
                    let prio = slot.object.priority();
                    self.emit_scheduler_record(QS_SCHED_NEXT, &[prio, 0]);
                    dispatched = slot.object.dispatch_one();
                }
                slot.executing_core.store(CORE_ID_NONE, Ordering::Release);
                return dispatched;
            }
        }
        false
    }

    /// Returns a clone of the kernel's QS trace hook, if any.
    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }
}

impl QvKernel {
    #[cfg(not(feature = "smp"))]
    fn new(config: KernelConfig, objects: ObjVec, trace: Option<TraceHook>, pubsub: Option<PubSubTable>) -> Self {
        #[cfg(not(feature = "static-alloc"))]
        let by_id = {
            let mut by_id = BTreeMap::new();
            for ao in &objects {
                by_id.insert(ao.id(), Arc::clone(ao));
            }
            by_id
        };
        Self {
            config,
            objects,
            #[cfg(not(feature = "static-alloc"))]
            by_id,
            trace,
            scheduler: Mutex::new(SchedulerState::default()),
            stop_flag: AtomicBool::new(false),
            pubsub,
        }
    }

    #[cfg(feature = "smp")]
    fn new(config: KernelConfig, objects: ObjVec, trace: Option<TraceHook>, pubsub: Option<PubSubTable>) -> Self {
        #[cfg(not(feature = "static-alloc"))]
        let mut by_id = BTreeMap::new();
        let mut slots = SlotVec::new();
        for ao in &objects {
            #[cfg(not(feature = "static-alloc"))]
            by_id.insert(ao.id(), clone_ref(ao));
            let slot = MpsActiveSlot {
                object: clone_ref(ao),
                executing_core: portable_atomic::AtomicU8::new(CORE_ID_NONE),
            };
            #[cfg(not(feature = "static-alloc"))]
            slots.push(slot);
            #[cfg(feature = "static-alloc")]
            if slots.push(slot).is_err() {
                crate::fusa::on_error(module_path!(), line!());
            }
        }
        Self {
            config,
            slots,
            #[cfg(not(feature = "static-alloc"))]
            by_id,
            trace,
            sched_ceiling: portable_atomic::AtomicU8::new(0),
            stop_flag: AtomicBool::new(false),
            pubsub,
        }
    }

    /// Subscribe the active object at `priority` to the given `signal`.
    pub fn subscribe(&self, signal: Signal, priority: u8) {
        if let Some(ref pubsub) = self.pubsub {
            pubsub.subscribe(signal, priority);
            self.emit_subscribe(priority, signal);
        }
    }

    /// Unsubscribe the active object at `priority` from the given `signal`.
    pub fn unsubscribe(&self, signal: Signal, priority: u8) {
        if let Some(ref pubsub) = self.pubsub {
            pubsub.unsubscribe(signal, priority);
            self.emit_unsubscribe(priority, signal);
        }
    }

    /// Unsubscribe the active object at `priority` from all signals.
    pub fn unsubscribe_all(&self, priority: u8) {
        if let Some(ref pubsub) = self.pubsub {
            pubsub.unsubscribe_all(priority);
        }
    }

    fn emit_scheduler_record(&self, record_type: u8, payload: &[u8]) {
        if let Some(trace) = &self.trace {
            let _ = trace(record_type, payload, true);
        }
    }

    fn emit_subscribe(&self, priority: u8, signal: Signal) {
        if let Some(trace) = &self.trace {
            let sig_bytes = signal.0.to_le_bytes();
            let _ = trace(12, &[priority, sig_bytes[0], sig_bytes[1]], true);
        }
    }

    fn emit_unsubscribe(&self, priority: u8, signal: Signal) {
        if let Some(trace) = &self.trace {
            let sig_bytes = signal.0.to_le_bytes();
            let _ = trace(13, &[priority, sig_bytes[0], sig_bytes[1]], true);
        }
    }

    fn emit_publish(&self, signal: Signal) {
        if let Some(trace) = &self.trace {
            let sig_bytes = signal.0.to_le_bytes();
            let _ = trace(26, &[sig_bytes[0], sig_bytes[1]], true);
        }
    }
}

impl QvKernel {
    /// Raises the scheduler ceiling to `ceiling`, suppressing dispatch of active
    /// objects at or below it until [`unlock_scheduler`](Self::unlock_scheduler).
    #[cfg(not(feature = "smp"))]
    pub fn lock_scheduler(&self, ceiling: u8) {
        let mut note: Option<[u8; 2]> = None;
        {
            let mut scheduler = self.scheduler.lock();
            if ceiling > scheduler.sched_ceiling {
                let prev = scheduler.sched_ceiling;
                scheduler.sched_ceiling = ceiling;
                note = Some([prev, ceiling]);
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_LOCK, &payload);
        }
    }

    /// Lowers the scheduler ceiling back to zero, re-enabling normal dispatch.
    #[cfg(not(feature = "smp"))]
    pub fn unlock_scheduler(&self) {
        let mut note: Option<[u8; 2]> = None;
        {
            let mut scheduler = self.scheduler.lock();
            if scheduler.sched_ceiling != 0 {
                let prev = scheduler.sched_ceiling;
                scheduler.sched_ceiling = 0;
                note = Some([prev, 0]);
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_UNLOCK, &payload);
        }
    }

    /// Raises the scheduler ceiling to `ceiling`, suppressing dispatch of active
    /// objects at or below it until [`unlock_scheduler`](Self::unlock_scheduler).
    #[cfg(feature = "smp")]
    pub fn lock_scheduler(&self, ceiling: u8) {
        let mut note: Option<[u8; 2]> = None;
        loop {
            let current = self.sched_ceiling.load(Ordering::Acquire);
            if ceiling <= current {
                break;
            }
            if self.sched_ceiling.compare_exchange(
                current,
                ceiling,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                note = Some([current, ceiling]);
                break;
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_LOCK, &payload);
        }
    }

    /// Lowers the scheduler ceiling back to zero, re-enabling normal dispatch.
    #[cfg(feature = "smp")]
    pub fn unlock_scheduler(&self) {
        let mut note: Option<[u8; 2]> = None;
        loop {
            let current = self.sched_ceiling.load(Ordering::Acquire);
            if current == 0 {
                break;
            }
            if self.sched_ceiling.compare_exchange(
                current,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            ).is_ok() {
                note = Some([current, 0]);
                break;
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_UNLOCK, &payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active::{new_active_object, ActiveContext, SignalHandler};
    use crate::event::DynEvent;
    use crate::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct MockBehavior {
        active_threads: Arc<Mutex<usize>>,
        max_concurrent_threads: Arc<Mutex<usize>>,
    }

    impl MockBehavior {
        fn new(active_threads: Arc<Mutex<usize>>, max_concurrent_threads: Arc<Mutex<usize>>) -> Self {
            Self { active_threads, max_concurrent_threads }
        }
    }

    impl SignalHandler for MockBehavior {
        fn handle_signal(&mut self, _signal: Signal, _ctx: &mut ActiveContext) {
            // Track thread entry
            {
                let mut active = self.active_threads.lock();
                *active += 1;
                let mut max = self.max_concurrent_threads.lock();
                if *active > *max {
                    *max = *active;
                }
            }

            // Simulate execution time
            std::thread::sleep(std::time::Duration::from_millis(5));

            // Track thread exit
            {
                let mut active = self.active_threads.lock();
                *active -= 1;
            }
        }
    }

    #[test]
    fn test_cooperative_dispatch() {
        let active_threads = Arc::new(Mutex::new(0));
        let max_concurrent_threads = Arc::new(Mutex::new(0));
        let ao_id = ActiveObjectId::new(10);
        let ao = new_active_object(
            ao_id,
            5,
            MockBehavior::new(Arc::clone(&active_threads), Arc::clone(&max_concurrent_threads)),
        );

        let kernel = QvKernel::builder().register(ao).build();
        kernel.start();

        kernel.post(ao_id, DynEvent::empty_dyn(Signal(42))).unwrap();
        assert!(kernel.has_pending_work());
        assert!(kernel.dispatch_once());
        assert!(!kernel.has_pending_work());

        assert_eq!(*max_concurrent_threads.lock(), 1);
    }

    #[test]
    #[cfg(feature = "smp")]
    fn test_smp_active_object_isolation_and_rtc() {
        let active_threads = Arc::new(Mutex::new(0));
        let max_concurrent_threads = Arc::new(Mutex::new(0));

        let ao_id = ActiveObjectId::new(1);
        let ao = new_active_object(
            ao_id,
            10,
            MockBehavior::new(Arc::clone(&active_threads), Arc::clone(&max_concurrent_threads))
        );

        let kernel = Arc::new(
            QvKernel::builder()
                .register(ao)
                .build()
        );

        kernel.start();

        // Queue multiple events to the SAME Active Object
        for i in 0..30 {
            kernel.post(ao_id, DynEvent::empty_dyn(Signal(i))).unwrap();
        }

        // Spawn multiple worker threads representing core runloops
        let mut handles = Vec::new();
        for _ in 0..4 {
            let kernel_clone = Arc::clone(&kernel);
            handles.push(std::thread::spawn(move || {
                while kernel_clone.has_pending_work() {
                    kernel_clone.dispatch_once();
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify that no two threads ever executed the behavior concurrently
        let max_concurrent = *max_concurrent_threads.lock();
        assert_eq!(max_concurrent, 1, "AO behavior was executed concurrently by multiple cores!");
    }
}

