//! Time event services (SRS §3.5).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::active::ActiveObjectId;
use crate::event::{DynEvent, Signal};
use crate::kernel::{Kernel, KernelError};
use qs::TraceHook;

#[derive(Debug, Clone)]
pub struct TimeEventConfig {
    pub signal: Signal,
    pub interval_ticks: Option<u64>,
}

impl TimeEventConfig {
    pub fn new(signal: Signal) -> Self {
        Self {
            signal,
            interval_ticks: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum TimeEventError {
    #[error(transparent)]
    Kernel(#[from] KernelError),
}

struct TimeEventInner {
    target: ActiveObjectId,
    cfg: TimeEventConfig,
    remaining: u64,
    armed: bool,
}

/// Software time event equivalent to `QTimeEvt`.
pub struct TimeEvent {
    inner: Mutex<TimeEventInner>,
    trace: Mutex<Option<TraceHook>>,
}

impl TimeEvent {
    pub fn new(target: ActiveObjectId, config: TimeEventConfig) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(TimeEventInner {
                target,
                cfg: config,
                remaining: 0,
                armed: false,
            }),
            trace: Mutex::new(None),
        })
    }

    pub fn arm(&self, timeout_ticks: u64, interval_ticks: Option<u64>) {
        let mut inner = self.inner.lock().unwrap();
        inner.remaining = timeout_ticks;
        inner.cfg.interval_ticks = interval_ticks;
        inner.armed = true;
    }

    pub fn disarm(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.armed = false;
        inner.remaining = 0;
    }

    pub fn is_armed(&self) -> bool {
        self.inner.lock().unwrap().armed
    }

    fn set_trace(&self, hook: Option<TraceHook>) {
        *self.trace.lock().unwrap() = hook;
    }

    fn tick(&self) -> Option<(ActiveObjectId, DynEvent)> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.armed {
            return None;
        }

        if inner.remaining > 0 {
            inner.remaining -= 1;
        }

        if inner.remaining == 0 {
            let target = inner.target;
            inner.armed = inner.cfg.interval_ticks.is_some();
            if let Some(period) = inner.cfg.interval_ticks {
                inner.remaining = period;
            }
            let event = DynEvent::empty_dyn(inner.cfg.signal);
            drop(inner);
            Some((target, event))
        } else {
            None
        }
    }
}

/// Cooperative timer wheel that calls into the kernel every tick.
pub struct TimerWheel {
    kernel: Arc<Kernel>,
    events: Vec<Arc<TimeEvent>>,
    trace: Option<TraceHook>,
}

impl TimerWheel {
    pub fn new(kernel: Arc<Kernel>) -> Self {
        let trace = kernel.trace_hook();
        Self {
            kernel,
            events: Vec::new(),
            trace,
        }
    }

    pub fn register(&mut self, event: Arc<TimeEvent>) {
        event.set_trace(self.trace.clone());
        self.events.push(event);
    }

    pub fn tick(&self) -> Result<(), TimeEventError> {
        for event in &self.events {
            if let Some((target, evt)) = event.tick() {
                self.kernel.post(target, evt.clone())?;
            }
        }
        Ok(())
    }
}
