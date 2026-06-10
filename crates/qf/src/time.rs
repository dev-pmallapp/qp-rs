//! Time event services (SRS §3.5).

use alloc::vec::Vec;
use core::fmt;

use crate::active::ActiveObjectId;
use crate::event::{DynEvent, Signal};
use crate::kernel::{Kernel, KernelError};
use crate::sync::{Arc, Mutex};
use crate::trace::TraceHook;

/// QS record: Time event armed with timeout and optional interval.
const QS_QF_TIMEEVT_ARM: u8 = 32;
/// QS record: One-shot time event auto-disarmed after firing.
const QS_QF_TIMEEVT_AUTO_DISARM: u8 = 33;
/// QS record: Attempted to disarm an already disarmed time event.
const QS_QF_TIMEEVT_DISARM_ATTEMPT: u8 = 34;
/// QS record: Time event successfully disarmed.
const QS_QF_TIMEEVT_DISARM: u8 = 35;
/// QS record: Time event counter updated via rearm() without disarm/rearm cycle.
const QS_QF_TIMEEVT_REARM: u8 = 36;
/// QS record: Time event posted to target active object.
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

    pub fn with_period(mut self, interval: u64) -> Self {
        self.interval_ticks = Some(interval);
        self
    }
}

#[derive(Debug)]
pub enum TimeEventError {
    Kernel(KernelError),
}

impl From<KernelError> for TimeEventError {
    fn from(value: KernelError) -> Self {
        Self::Kernel(value)
    }
}

impl fmt::Display for TimeEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel(err) => write!(f, "kernel error: {err}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TimeEventError {}

struct TimeEventInner {
    target: ActiveObjectId,
    cfg: TimeEventConfig,
    remaining: u64,
    armed: bool,
    /// Sticky "was disarmed" flag — set when a one-shot fires or `disarm()`
    /// is called.  Cleared (and value returned) by `was_disarmed()`.
    disarmed_flag: bool,
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
                disarmed_flag: false,
            }),
            trace: Mutex::new(None),
            meta: Mutex::new(None),
        })
    }

    pub fn arm(&self, timeout_ticks: u64, interval_ticks: Option<u64>) {
        let mut inner = self.inner.lock();
        inner.remaining = timeout_ticks;
        inner.cfg.interval_ticks = interval_ticks;
        inner.armed = true;
        drop(inner);

        self.emit_arm(timeout_ticks, interval_ticks.unwrap_or(0));
    }

    pub fn disarm(&self) {
        let mut inner = self.inner.lock();
        if inner.armed {
            let remaining = inner.remaining;
            let interval = inner.cfg.interval_ticks.unwrap_or(0);
            inner.armed = false;
            inner.remaining = 0;
            inner.disarmed_flag = true;
            drop(inner);
            self.emit_disarm(remaining, interval);
        } else {
            drop(inner);
            self.emit_disarm_attempt();
        }
    }

    /// Update the expiry counter without a full disarm/rearm cycle.
    ///
    /// If the time event is **armed**, the counter is updated and the
    /// function returns `true` (was armed).  If the time event is
    /// **disarmed**, it is armed with the new timeout and returns `false`
    /// (was not armed).
    ///
    /// Corresponds to `QTimeEvt::rearm()` in QP/C++.
    pub fn rearm(&self, timeout_ticks: u64) -> bool {
        let mut inner = self.inner.lock();
        let was_armed = inner.armed;
        inner.remaining = timeout_ticks;
        inner.armed = true;
        let interval = inner.cfg.interval_ticks.unwrap_or(0);
        drop(inner);
        self.emit_rearm(timeout_ticks, interval);
        was_armed
    }

    /// Returns and clears the sticky "was disarmed" flag.
    ///
    /// The flag is set when:
    /// - A one-shot time event fires (auto-disarm), or
    /// - `disarm()` is called on an armed event.
    ///
    /// Corresponds to `QTimeEvt::wasDisarmed()` in QP/C++.
    pub fn was_disarmed(&self) -> bool {
        let mut inner = self.inner.lock();
        let flag = inner.disarmed_flag;
        inner.disarmed_flag = false;
        flag
    }

    pub fn is_armed(&self) -> bool {
        self.inner.lock().armed
    }

    pub fn set_trace(&self, hook: Option<TraceHook>) {
        *self.trace.lock() = hook;
    }

    pub fn set_trace_meta(&self, info: TimeEventTraceInfo) {
        *self.meta.lock() = Some(info);
    }

    pub fn poll(&self) -> Option<(ActiveObjectId, DynEvent)> {
        let mut inner = self.inner.lock();
        if !inner.armed {
            return None;
        }

        if inner.remaining > 0 {
            inner.remaining -= 1;
        }

        if inner.remaining == 0 {
            let target = inner.target;
            let periodic = inner.cfg.interval_ticks.is_some();
            inner.armed = periodic;
            if let Some(period) = inner.cfg.interval_ticks {
                inner.remaining = period;
            }
            if !periodic {
                inner.disarmed_flag = true;
            }
            let signal = inner.cfg.signal;
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

    /// Advance the timer wheel from an ISR context.
    ///
    /// Semantically identical to `tick()` but signals intent that the caller
    /// is inside an interrupt service routine (`qk_isr_entry!()` was called).
    ///
    /// Corresponds to `QTimeEvt::tickFromISR_()` in QP/C++.
    pub fn tick_from_isr(&self) -> Result<(), TimeEventError> {
        debug_assert!(
            crate::isr::in_isr(),
            "tick_from_isr called outside ISR context"
        );
        self.tick()
    }
}

impl TimeEvent {
    fn obtain_trace(&self) -> Option<(TraceHook, TimeEventTraceInfo)> {
        let trace = self.trace.lock().clone()?;
        let meta = self.meta.lock().clone()?;
        Some((trace, meta))
    }

    /// Helper to emit a trace record with a payload built on the stack.
    ///
    /// Uses a fixed-size buffer to avoid heap allocations in hot paths.
    fn emit_trace<F>(&self, record: u8, timestamp: bool, builder: F)
    where
        F: FnOnce(&mut [u8]) -> usize,
    {
        if let Some((trace, _)) = self.obtain_trace() {
            let mut buf = [0u8; 32];
            let len = builder(&mut buf);
            let _ = trace(record, &buf[..len], timestamp);
        }
    }

    fn emit_arm(&self, n_ticks: u64, interval: u64) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_ARM, true, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(n_ticks).to_le_bytes());
                pos += 2;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(interval).to_le_bytes());
                pos += 2;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }

    fn emit_disarm(&self, remaining: u64, interval: u64) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_DISARM, true, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(remaining).to_le_bytes());
                pos += 2;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(interval).to_le_bytes());
                pos += 2;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }

    fn emit_rearm(&self, n_ticks: u64, interval: u64) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_REARM, true, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(n_ticks).to_le_bytes());
                pos += 2;
                buf[pos..pos + 2].copy_from_slice(&truncate_u16(interval).to_le_bytes());
                pos += 2;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }

    fn emit_disarm_attempt(&self) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_DISARM_ATTEMPT, true, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }

    fn emit_auto_disarm(&self) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_AUTO_DISARM, false, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }

    fn emit_post(&self, signal: Signal) {
        if let Some((_, meta)) = self.obtain_trace() {
            self.emit_trace(QS_QF_TIMEEVT_POST, true, |buf| {
                let mut pos = 0;
                buf[pos..pos + 8].copy_from_slice(&meta.time_event_addr.to_le_bytes());
                pos += 8;
                buf[pos..pos + 2].copy_from_slice(&signal.0.to_le_bytes());
                pos += 2;
                buf[pos..pos + 8].copy_from_slice(&meta.target_addr.to_le_bytes());
                pos += 8;
                buf[pos] = meta.tick_rate;
                pos + 1
            });
        }
    }
}

fn truncate_u16(value: u64) -> u16 {
    value.min(u16::MAX as u64) as u16
}
