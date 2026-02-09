//! Cooperative kernel and scheduling services (SRS §3.4).

use core::fmt;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::sync::{Arc, Mutex};
use crate::trace::{TraceError, TraceHook};

use crate::active::{ActiveObjectId, ActiveObjectRef};
use crate::event::{DynEvent, Signal};

const QS_SCHED_LOCK: u8 = 50;
const QS_SCHED_UNLOCK: u8 = 51;
const QS_SCHED_NEXT: u8 = 52;
const QS_SCHED_IDLE: u8 = 53;

#[derive(Default)]
struct SchedulerState {
    prev_prio: u8,
    sched_ceiling: u8,
}

#[derive(Debug, Clone)]
pub struct KernelConfig {
    pub name: &'static str,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self { name: "QP" }
    }
}

pub struct KernelBuilder {
    config: KernelConfig,
    objects: Vec<ActiveObjectRef>,
    trace: Option<TraceHook>,
}

impl KernelBuilder {
    pub fn new(config: KernelConfig) -> Self {
        Self {
            config,
            objects: Vec::new(),
            trace: None,
        }
    }

    pub fn register(mut self, object: ActiveObjectRef) -> Self {
        self.objects.push(object);
        self
    }

    pub fn with_trace_hook(mut self, hook: TraceHook) -> Self {
        self.trace = Some(hook);
        self
    }

    pub fn build(mut self) -> Kernel {
        self.objects.sort_by_key(|ao| ao.priority());
        Kernel::new(self.config, self.objects, self.trace)
    }
}

#[derive(Debug)]
pub enum KernelError {
    NotFound(ActiveObjectId),
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

pub struct Kernel {
    config: KernelConfig,
    objects: Vec<ActiveObjectRef>,
    by_id: BTreeMap<ActiveObjectId, ActiveObjectRef>,
    trace: Option<TraceHook>,
    scheduler: Mutex<SchedulerState>,
}

impl Kernel {
    pub fn builder() -> KernelBuilder {
        KernelBuilder::new(KernelConfig::default())
    }

    pub fn with_config(config: KernelConfig) -> KernelBuilder {
        KernelBuilder::new(config)
    }

    pub fn post(&self, target: ActiveObjectId, event: DynEvent) -> Result<(), KernelError> {
        if let Some(ao) = self.by_id.get(&target) {
            ao.post(event);
            Ok(())
        } else {
            Err(KernelError::NotFound(target))
        }
    }

    pub fn publish(&self, signal: Signal, event: DynEvent) {
        for ao in &self.objects {
            // Basic publish duplicates the event header, but payload is shared via Arc.
            let mut cloned = event.clone();
            cloned.header.signal = signal;
            ao.post(cloned);
        }
    }

    pub fn start(&self) {
        for ao in &self.objects {
            ao.start(self.trace.clone());
        }
    }

    pub fn run_until_idle(&self) {
        while self.dispatch_once() {}
    }

    pub fn dispatch_once(&self) -> bool {
        let candidate = self
            .objects
            .iter()
            .rev()
            .find(|ao| ao.has_events())
            .cloned();

        if let Some(ao) = candidate {
            let mut note = None;
            let mut should_dispatch = true;

            {
                let mut scheduler = self.scheduler.lock();
                let prio = ao.priority();
                if prio <= scheduler.sched_ceiling {
                    should_dispatch = false;
                    if scheduler.prev_prio != 0 {
                        let prev = scheduler.prev_prio;
                        scheduler.prev_prio = 0;
                        note = Some((QS_SCHED_IDLE, vec![prev]));
                    }
                } else {
                    let prev = scheduler.prev_prio;
                    if prio != prev {
                        note = Some((QS_SCHED_NEXT, vec![prio, prev]));
                    }
                    scheduler.prev_prio = prio;
                }
            }

            if let Some((record, payload)) = note {
                self.emit_scheduler_record(record, payload);
            }

            if !should_dispatch {
                return false;
            }

            ao.dispatch_one()
        } else {
            let mut note = None;
            {
                let mut scheduler = self.scheduler.lock();
                if scheduler.prev_prio != 0 {
                    let prev = scheduler.prev_prio;
                    scheduler.prev_prio = 0;
                    note = Some((QS_SCHED_IDLE, vec![prev]));
                }
            }

            if let Some((record, payload)) = note {
                self.emit_scheduler_record(record, payload);
            }

            false
        }
    }

    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }
}

impl Kernel {
    fn new(config: KernelConfig, objects: Vec<ActiveObjectRef>, trace: Option<TraceHook>) -> Self {
        let mut by_id = BTreeMap::new();
        for ao in &objects {
            by_id.insert(ao.id(), Arc::clone(ao));
        }
        Self {
            config,
            objects,
            by_id,
            trace,
            scheduler: Mutex::new(SchedulerState::default()),
        }
    }

    fn emit_scheduler_record(&self, record_type: u8, payload: Vec<u8>) {
        if let Some(trace) = &self.trace {
            let _ = trace(record_type, &payload, true);
        }
    }
}

impl Kernel {
    pub fn lock_scheduler(&self, ceiling: u8) {
        let mut note = None;
        {
            let mut scheduler = self.scheduler.lock();
            if ceiling > scheduler.sched_ceiling {
                let prev = scheduler.sched_ceiling;
                scheduler.sched_ceiling = ceiling;
                note = Some(vec![prev, ceiling]);
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_LOCK, payload);
        }
    }

    pub fn unlock_scheduler(&self) {
        let mut note = None;
        {
            let mut scheduler = self.scheduler.lock();
            if scheduler.sched_ceiling != 0 {
                let prev = scheduler.sched_ceiling;
                scheduler.sched_ceiling = 0;
                note = Some(vec![prev, 0]);
            }
        }

        if let Some(payload) = note {
            self.emit_scheduler_record(QS_SCHED_UNLOCK, payload);
        }
    }
}
