//! Cooperative kernel and scheduling services (SRS §3.4).

use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;

use crate::active::{ActiveObjectId, ActiveObjectRef};
use crate::event::{DynEvent, Signal};
use qs::{TraceError, TraceHook};

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

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("active object {0:?} not found")]
    NotFound(ActiveObjectId),
    #[error("trace error: {0}")]
    Trace(#[from] TraceError),
}

pub struct Kernel {
    config: KernelConfig,
    objects: Vec<ActiveObjectRef>,
    by_id: HashMap<ActiveObjectId, ActiveObjectRef>,
    trace: Option<TraceHook>,
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
        for ao in self.objects.iter().rev() {
            if ao.dispatch_one() {
                return true;
            }
        }
        false
    }

    pub fn trace_hook(&self) -> Option<TraceHook> {
        self.trace.clone()
    }
}

impl Kernel {
    fn new(config: KernelConfig, objects: Vec<ActiveObjectRef>, trace: Option<TraceHook>) -> Self {
        let mut by_id = HashMap::new();
        for ao in &objects {
            by_id.insert(ao.id(), Arc::clone(ao));
        }
        Self {
            config,
            objects,
            by_id,
            trace,
        }
    }
}
