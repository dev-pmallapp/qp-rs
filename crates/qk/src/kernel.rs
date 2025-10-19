use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use qf::active::{ActiveObjectId, ActiveObjectRef};
use qf::event::{DynEvent, Signal};
use qs::TraceHook;

use crate::scheduler::{QkScheduler, SchedStatus, ScheduleDecision};

const MAX_PRIORITY: usize = 63;

#[derive(Clone)]
struct Registration {
    object: ActiveObjectRef,
    priority: u8,
    threshold: u8,
    id: ActiveObjectId,
}

#[derive(Clone)]
struct ActiveSlot {
    object: ActiveObjectRef,
    threshold: u8,
}

pub struct QkKernelBuilder {
    registrations: Vec<Registration>,
    trace: Option<TraceHook>,
}

impl QkKernelBuilder {
    pub fn new() -> Self {
        Self {
            registrations: Vec::new(),
            trace: None,
        }
    }

    pub fn register(mut self, object: ActiveObjectRef) -> Self {
        let priority = object.priority();
        assert!(priority > 0, "priority 0 is reserved for the idle thread");
        assert!(
            priority as usize <= MAX_PRIORITY,
            "priority {priority} exceeds supported range"
        );
        let id = object.id();
        self.registrations.push(Registration {
            threshold: priority,
            priority,
            id,
            object,
        });
        self
    }

    pub fn register_with_threshold(mut self, object: ActiveObjectRef, threshold: u8) -> Self {
        let priority = object.priority();
        assert!(priority > 0, "priority 0 is reserved for the idle thread");
        assert!(
            priority as usize <= MAX_PRIORITY,
            "priority {priority} exceeds supported range"
        );
        assert!(
            threshold >= priority,
            "preemption threshold must be >= priority"
        );
        assert!(
            threshold as usize <= MAX_PRIORITY,
            "threshold {threshold} exceeds supported range"
        );
        let id = object.id();
        self.registrations.push(Registration {
            threshold,
            priority,
            id,
            object,
        });
        self
    }

    pub fn with_trace_hook(mut self, hook: TraceHook) -> Self {
        self.trace = Some(hook);
        self
    }

    pub fn build(self) -> Result<QkKernel, QkKernelError> {
        QkKernel::new(self.registrations, self.trace)
    }
}

#[derive(Debug)]
pub enum QkKernelError {
    DuplicatePriority(u8),
    NotFound(ActiveObjectId),
}

impl fmt::Display for QkKernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePriority(prio) => {
                write!(f, "active object priority {prio} already registered")
            }
            Self::NotFound(id) => write!(f, "active object {id:?} not found"),
        }
    }
}

impl std::error::Error for QkKernelError {}

pub struct QkKernel {
    scheduler: Arc<QkScheduler>,
    slots: Vec<Option<ActiveSlot>>,
    id_to_prio: HashMap<ActiveObjectId, u8>,
    trace: Option<TraceHook>,
}

impl QkKernel {
    pub fn builder() -> QkKernelBuilder {
        QkKernelBuilder::new()
    }

    fn new(
        registrations: Vec<Registration>,
        trace: Option<TraceHook>,
    ) -> Result<Self, QkKernelError> {
        let mut slots: Vec<Option<ActiveSlot>> = vec![None; MAX_PRIORITY + 1];
        let mut id_to_prio = HashMap::new();

        for registration in registrations {
            let prio = registration.priority as usize;
            if slots[prio].is_some() {
                return Err(QkKernelError::DuplicatePriority(registration.priority));
            }
            id_to_prio.insert(registration.id, registration.priority);
            slots[prio] = Some(ActiveSlot {
                object: registration.object,
                threshold: registration.threshold,
            });
        }

        let scheduler = Arc::new(QkScheduler::new(trace.clone()));
        scheduler.configure_active(0, 0);

        Ok(Self {
            scheduler,
            slots,
            id_to_prio,
            trace,
        })
    }

    pub fn scheduler(&self) -> Arc<QkScheduler> {
        Arc::clone(&self.scheduler)
    }

    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }

    pub fn lock_scheduler(&self, ceiling: u8) -> SchedStatus {
        self.scheduler.lock(ceiling)
    }

    pub fn unlock_scheduler(&self, status: SchedStatus) {
        let should_activate = matches!(status, SchedStatus::Locked(_));
        self.scheduler.unlock(status);

        if should_activate {
            if let Some(decision) = self.scheduler.plan_activation() {
                self.activate(decision);
            }
        }
    }

    pub fn start(&self) {
        for slot in self.slots.iter().flatten() {
            slot.object.start(self.trace.clone());
            if slot.object.has_events() {
                self.scheduler.mark_ready(slot.object.priority());
            }
        }
    }

    pub fn post(&self, target: ActiveObjectId, event: DynEvent) -> Result<(), QkKernelError> {
        let prio = self
            .id_to_prio
            .get(&target)
            .copied()
            .ok_or(QkKernelError::NotFound(target))?;
        let slot = self.slots[prio as usize]
            .as_ref()
            .expect("kernel registry invariant broken");
        let was_empty = !slot.object.has_events();
        slot.object.post(event);
        if was_empty {
            self.scheduler.mark_ready(prio);
        }
        Ok(())
    }

    pub fn post_and_run(
        &self,
        target: ActiveObjectId,
        event: DynEvent,
    ) -> Result<(), QkKernelError> {
        self.post(target, event)?;
        self.run_until_idle();
        Ok(())
    }

    pub fn publish(&self, signal: Signal, event: DynEvent) {
        for (prio, slot_opt) in self.slots.iter().enumerate() {
            if let Some(slot) = slot_opt {
                let was_empty = !slot.object.has_events();
                let mut cloned = event.clone();
                cloned.header.signal = signal;
                slot.object.post(cloned);
                if was_empty {
                    self.scheduler.mark_ready(prio as u8);
                }
            }
        }
    }

    pub fn publish_and_run(&self, signal: Signal, event: DynEvent) {
        self.publish(signal, event);
        self.run_until_idle();
    }

    pub fn dispatch_once(&self) -> bool {
        match self.scheduler.plan_activation() {
            Some(decision) => {
                self.activate(decision);
                true
            }
            None => false,
        }
    }

    pub fn run_until_idle(&self) {
        while self.dispatch_once() {}
    }

    pub fn has_pending_work(&self) -> bool {
        self.scheduler.has_ready_to_run()
    }

    fn activate(&self, initial: ScheduleDecision) {
        let restore_prio = initial.previous_prio;
        let restore_threshold = self.threshold_for(restore_prio);
        let mut decision = initial;

        loop {
            let (object, threshold) = {
                let slot = self.slots[decision.next_prio as usize]
                    .as_ref()
                    .expect("scheduled priority not registered");
                (Arc::clone(&slot.object), slot.threshold)
            };

            self.scheduler.commit_activation(&decision, threshold);

            let processed = object.dispatch_one();
            debug_assert!(processed, "scheduled active object had no event");

            if !object.has_events() {
                self.scheduler.mark_not_ready(decision.next_prio);
            }

            match self.scheduler.next_after_dispatch(restore_threshold) {
                Some(next) => {
                    decision = next;
                }
                None => {
                    self.scheduler
                        .restore_active(restore_prio, restore_threshold);
                    break;
                }
            }
        }
    }

    fn threshold_for(&self, prio: u8) -> u8 {
        if prio == 0 {
            0
        } else {
            self.slots[prio as usize]
                .as_ref()
                .map(|slot| slot.threshold)
                .expect("missing preemption threshold for registered AO")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::scheduler::SchedStatus;
    use qf::active::{new_active_object, ActiveContext, SignalHandler};

    #[derive(Clone)]
    struct Recorder {
        id: ActiveObjectId,
        log: Arc<Mutex<Vec<(ActiveObjectId, Signal)>>>,
    }

    impl Recorder {
        fn new(id: ActiveObjectId, log: Arc<Mutex<Vec<(ActiveObjectId, Signal)>>>) -> Self {
            Self { id, log }
        }
    }

    impl SignalHandler for Recorder {
        fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActiveContext) {
            self.log.lock().unwrap().push((self.id, signal));
        }
    }

    #[test]
    fn schedules_highest_priority_first() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let low_id = ActiveObjectId::new(1);
        let high_id = ActiveObjectId::new(2);

        let low = new_active_object(low_id, 2, Recorder::new(low_id, Arc::clone(&log)));
        let high = new_active_object(high_id, 5, Recorder::new(high_id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register(low)
            .register(high)
            .build()
            .expect("kernel should build");

        kernel.start();

        kernel
            .post(low_id, DynEvent::empty_dyn(Signal(1)))
            .expect("low prio post");
        kernel
            .post(high_id, DynEvent::empty_dyn(Signal(2)))
            .expect("high prio post");

        kernel.run_until_idle();

        let events = log.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], (high_id, Signal(2)));
        assert_eq!(events[1], (low_id, Signal(1)));
    }

    #[test]
    fn preemption_threshold_blocks_lower_priorities() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let base_id = ActiveObjectId::new(1);
        let mid_id = ActiveObjectId::new(2);
        let high_id = ActiveObjectId::new(3);

        let base_prio = 2;
        let mid_prio = 3;
        let high_prio = 6;

        let base = new_active_object(base_id, base_prio, Recorder::new(base_id, Arc::clone(&log)));
        let mid = new_active_object(mid_id, mid_prio, Recorder::new(mid_id, Arc::clone(&log)));
        let high = new_active_object(high_id, high_prio, Recorder::new(high_id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register_with_threshold(base, 4)
            .register(mid)
            .register(high)
            .build()
            .expect("kernel should build");

        kernel.start();

        kernel.scheduler().configure_active(base_prio, 4);

        kernel
            .post(mid_id, DynEvent::empty_dyn(Signal(1)))
            .expect("mid prio post");
        kernel
            .post(high_id, DynEvent::empty_dyn(Signal(2)))
            .expect("high prio post");

        assert!(kernel.dispatch_once(), "high priority should preempt");

        {
            let entries = log.lock().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0], (high_id, Signal(2)));
        }

        assert!(kernel.scheduler().is_ready(mid_prio));
    }

    #[test]
    fn post_and_run_dispatches_event() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(4);
        let ao = new_active_object(ao_id, 3, Recorder::new(ao_id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register(ao)
            .build()
            .expect("kernel should build");

        kernel.start();

        kernel
            .post_and_run(ao_id, DynEvent::empty_dyn(Signal(9)))
            .expect("post should succeed");

        let entries = log.lock().unwrap();
        assert_eq!(entries.as_slice(), &[(ao_id, Signal(9))]);
    }

    #[test]
    fn unlock_scheduler_triggers_pending_work() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let high_id = ActiveObjectId::new(5);
        let high = new_active_object(high_id, 6, Recorder::new(high_id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register(high)
            .build()
            .expect("kernel should build");

        kernel.start();

        let status = kernel.lock_scheduler(6);
        assert!(matches!(status, SchedStatus::Locked(_)));

        kernel
            .post(high_id, DynEvent::empty_dyn(Signal(11)))
            .expect("post should succeed");

        assert!(
            !kernel.dispatch_once(),
            "lock ceiling should block scheduling"
        );

        kernel.unlock_scheduler(status);

        let entries = log.lock().unwrap();
        assert_eq!(entries.as_slice(), &[(high_id, Signal(11))]);
        assert!(!kernel.scheduler().is_ready(6));
    }

    #[test]
    fn publish_and_run_delivers_to_all_subscribers() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let low_id = ActiveObjectId::new(6);
        let high_id = ActiveObjectId::new(7);

        let low = new_active_object(low_id, 2, Recorder::new(low_id, Arc::clone(&log)));
        let high = new_active_object(high_id, 5, Recorder::new(high_id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register(low)
            .register(high)
            .build()
            .expect("kernel should build");

        kernel.start();

        kernel.publish_and_run(Signal(42), DynEvent::empty_dyn(Signal(0)));

        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], (high_id, Signal(42)));
        assert_eq!(entries[1], (low_id, Signal(42)));
    }

    #[test]
    fn has_pending_work_tracks_ready_set() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let id = ActiveObjectId::new(8);
        let ao = new_active_object(id, 4, Recorder::new(id, Arc::clone(&log)));

        let kernel = QkKernel::builder()
            .register(ao)
            .build()
            .expect("kernel should build");

        kernel.start();
        assert!(!kernel.has_pending_work());

        kernel
            .post(id, DynEvent::empty_dyn(Signal(1)))
            .expect("post should succeed");
        assert!(kernel.has_pending_work());

        kernel.run_until_idle();
        assert!(!kernel.has_pending_work());
    }
}
