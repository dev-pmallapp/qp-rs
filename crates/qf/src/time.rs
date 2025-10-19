//! Time event services (SRS §3.5).

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::active::ActiveObjectId;
use crate::event::{DynEvent, Signal};
use crate::kernel::{Kernel, KernelError};
use qs::TraceHook;

const QS_QF_TIMEEVT_ARM: u8 = 32;
const QS_QF_TIMEEVT_AUTO_DISARM: u8 = 33;
const QS_QF_TIMEEVT_DISARM_ATTEMPT: u8 = 34;
const QS_QF_TIMEEVT_DISARM: u8 = 35;
const QS_QF_TIMEEVT_POST: u8 = 37;

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
    meta: Mutex<Option<TimeEventTraceInfo>>,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeEventTraceInfo {
    pub time_event_addr: u64,
    pub target_addr: u64,
    pub tick_rate: u8,
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
            meta: Mutex::new(None),
        })
    }

    pub fn arm(&self, timeout_ticks: u64, interval_ticks: Option<u64>) {
        let mut inner = self.inner.lock().unwrap();
        inner.remaining = timeout_ticks;
        inner.cfg.interval_ticks = interval_ticks;
        inner.armed = true;
        drop(inner);

        self.emit_arm(timeout_ticks, interval_ticks.unwrap_or(0));
    }

    pub fn disarm(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.armed {
            let remaining = inner.remaining;
            let interval = inner.cfg.interval_ticks.unwrap_or(0);
            inner.armed = false;
            inner.remaining = 0;
            drop(inner);
            self.emit_disarm(remaining, interval);
        } else {
            drop(inner);
            self.emit_disarm_attempt();
        }
    }

    pub fn is_armed(&self) -> bool {
        self.inner.lock().unwrap().armed
    }

    pub fn set_trace(&self, hook: Option<TraceHook>) {
        *self.trace.lock().unwrap() = hook;
    }

    pub fn set_trace_meta(&self, info: TimeEventTraceInfo) {
        *self.meta.lock().unwrap() = Some(info);
    }

    pub fn poll(&self) -> Option<(ActiveObjectId, DynEvent)> {
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
            let signal = inner.cfg.signal;
            let periodic = inner.cfg.interval_ticks.is_some();
            drop(inner);
            if !periodic {
                self.emit_auto_disarm();
            }
            self.emit_post(signal);
            Some((target, DynEvent::empty_dyn(signal)))
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
            if let Some((target, evt)) = event.poll() {
                self.kernel.post(target, evt.clone())?;
            }
        }
        Ok(())
    }
}

impl TimeEvent {
    fn obtain_trace(&self) -> Option<(TraceHook, TimeEventTraceInfo)> {
        let trace = self.trace.lock().unwrap().clone()?;
        let meta = self.meta.lock().unwrap().clone()?;
        Some((trace, meta))
    }

    fn emit_arm(&self, n_ticks: u64, interval: u64) {
        if let Some((trace, meta)) = self.obtain_trace() {
            let mut payload = Vec::with_capacity(8 + 8 + 2 + 2 + 1);
            payload.extend_from_slice(&meta.time_event_addr.to_le_bytes());
            payload.extend_from_slice(&meta.target_addr.to_le_bytes());
            payload.extend_from_slice(&(truncate_u16(n_ticks).to_le_bytes()));
            payload.extend_from_slice(&(truncate_u16(interval).to_le_bytes()));
            payload.push(meta.tick_rate);
            if let Err(err) = trace(QS_QF_TIMEEVT_ARM, &payload, true) {
                eprintln!("failed to emit QS_QF_TIMEEVT_ARM: {err}");
            }
        }
    }

    fn emit_disarm(&self, remaining: u64, interval: u64) {
        if let Some((trace, meta)) = self.obtain_trace() {
            let mut payload = Vec::with_capacity(8 + 8 + 2 + 2 + 1);
            payload.extend_from_slice(&meta.time_event_addr.to_le_bytes());
            payload.extend_from_slice(&meta.target_addr.to_le_bytes());
            payload.extend_from_slice(&(truncate_u16(remaining).to_le_bytes()));
            payload.extend_from_slice(&(truncate_u16(interval).to_le_bytes()));
            payload.push(meta.tick_rate);
            if let Err(err) = trace(QS_QF_TIMEEVT_DISARM, &payload, true) {
                eprintln!("failed to emit QS_QF_TIMEEVT_DISARM: {err}");
            }
        }
    }

    fn emit_disarm_attempt(&self) {
        if let Some((trace, meta)) = self.obtain_trace() {
            let mut payload = Vec::with_capacity(8 + 8 + 1);
            payload.extend_from_slice(&meta.time_event_addr.to_le_bytes());
            payload.extend_from_slice(&meta.target_addr.to_le_bytes());
            payload.push(meta.tick_rate);
            if let Err(err) = trace(QS_QF_TIMEEVT_DISARM_ATTEMPT, &payload, true) {
                eprintln!("failed to emit QS_QF_TIMEEVT_DISARM_ATTEMPT: {err}");
            }
        }
    }

    fn emit_auto_disarm(&self) {
        if let Some((trace, meta)) = self.obtain_trace() {
            let mut payload = Vec::with_capacity(8 + 8 + 1);
            payload.extend_from_slice(&meta.time_event_addr.to_le_bytes());
            payload.extend_from_slice(&meta.target_addr.to_le_bytes());
            payload.push(meta.tick_rate);
            if let Err(err) = trace(QS_QF_TIMEEVT_AUTO_DISARM, &payload, false) {
                eprintln!("failed to emit QS_QF_TIMEEVT_AUTO_DISARM: {err}");
            }
        }
    }

    fn emit_post(&self, signal: Signal) {
        if let Some((trace, meta)) = self.obtain_trace() {
            let mut payload = Vec::with_capacity(8 + 2 + 8 + 1);
            payload.extend_from_slice(&meta.time_event_addr.to_le_bytes());
            payload.extend_from_slice(&signal.0.to_le_bytes());
            payload.extend_from_slice(&meta.target_addr.to_le_bytes());
            payload.push(meta.tick_rate);
            if let Err(err) = trace(QS_QF_TIMEEVT_POST, &payload, true) {
                eprintln!("failed to emit QS_QF_TIMEEVT_POST: {err}");
            }
        }
    }
}

fn truncate_u16(value: u64) -> u16 {
    value.min(u16::MAX as u64) as u16
}
