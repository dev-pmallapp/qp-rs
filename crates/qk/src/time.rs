use alloc::vec::Vec;
use core::fmt;

use qf::time::TimeEvent;
use qf::TraceHook;

use crate::kernel::{QkKernel, QkKernelError};
use crate::sync::Arc;

/// Timer wheel that polls registered [`TimeEvent`]s and posts expired events to
/// their target active objects through the [`QkKernel`].
pub struct QkTimerWheel {
    kernel: Arc<QkKernel>,
    events: Vec<Vec<Arc<TimeEvent>>>,
    trace: Option<TraceHook>,
}

impl QkTimerWheel {
    /// Creates a timer wheel bound to the given kernel, inheriting its trace hook.
    pub fn new(kernel: Arc<QkKernel>) -> Self {
        let trace = kernel.trace_hook();
        // Fallback size to 1 if no registered tick rates, or use standard count
        let mut events = Vec::with_capacity(4);
        for _ in 0..4 {
            events.push(Vec::new());
        }
        Self {
            kernel,
            events,
            trace,
        }
    }

    /// Registers a time event with the wheel, wiring up the wheel's trace hook.
    pub fn register(&mut self, event: Arc<TimeEvent>) {
        event.set_trace(self.trace.clone());
        let rate = event.tick_rate() as usize;
        if rate < self.events.len() {
            self.events[rate].push(event);
        } else {
            while self.events.len() <= rate {
                self.events.push(Vec::new());
            }
            self.events[rate].push(event);
        }
    }

    /// Advances the wheel for the specified `tick_rate` domain by one tick, posting any events that have expired.
    pub fn tick_rate(&self, tick_rate: u8) -> Result<(), QkTimeEventError> {
        let rate = tick_rate as usize;
        if rate < self.events.len() {
            for event in &self.events[rate] {
                if let Some((target, evt)) = event.poll() {
                    self.kernel.post_and_run(target, evt)?;
                }
            }
        }
        Ok(())
    }

    /// Advances the default (tick_rate 0) wheel by one tick.
    pub fn tick(&self) -> Result<(), QkTimeEventError> {
        self.tick_rate(0)
    }

    /// Advance the timer wheel for the specified `tick_rate` domain from an ISR context.
    ///
    /// Caller must have called `qf::qk_isr_entry!()` before this.
    /// Corresponds to `QTimeEvt::tickFromISR_()` in QP/C++.
    pub fn tick_rate_from_isr(&self, tick_rate: u8) -> Result<(), QkTimeEventError> {
        debug_assert!(
            qf::isr::in_isr(),
            "tick_rate_from_isr called outside ISR context"
        );
        self.tick_rate(tick_rate)
    }

    /// Advance the default timer wheel from an ISR context.
    ///
    /// Caller must have called `qf::qk_isr_entry!()` before this.
    /// Corresponds to `QTimeEvt::tickFromISR_()` in QP/C++.
    pub fn tick_from_isr(&self) -> Result<(), QkTimeEventError> {
        debug_assert!(
            qf::isr::in_isr(),
            "tick_from_isr called outside ISR context"
        );
        self.tick_rate(0)
    }

    /// Returns `true` if there are no armed time events in the specified `tick_rate` domain.
    pub fn no_active(&self, tick_rate: u8) -> bool {
        let rate = tick_rate as usize;
        if rate < self.events.len() {
            for event in &self.events[rate] {
                if event.is_armed() {
                    return false;
                }
            }
        }
        true
    }
}

/// Errors that can occur while ticking the QK timer wheel.
#[derive(Debug)]
pub enum QkTimeEventError {
    /// The kernel rejected the posting of an expired time event.
    Kernel(QkKernelError),
}

impl fmt::Display for QkTimeEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel(err) => write!(f, "time event kernel error: {err}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for QkTimeEventError {}

impl From<QkKernelError> for QkTimeEventError {
    fn from(value: QkKernelError) -> Self {
        Self::Kernel(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::Arc;
    use std::sync::Mutex;

    use qf::active::{
        new_active_object, ActiveContext, ActiveObjectId, ActiveObjectRef, SignalHandler,
    };
    use qf::event::Signal;
    use qf::time::{TimeEvent, TimeEventConfig, TimeEventTraceInfo};

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

    fn build_kernel(object: ActiveObjectRef) -> Arc<QkKernel> {
        QkKernel::builder()
            .register(object)
            .expect("register should succeed")
            .build()
            .map(Arc::new)
            .expect("kernel should build")
    }

    #[test]
    fn one_shot_time_event_dispatches() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(10);
        let ao = new_active_object(ao_id, 4, Recorder::new(ao_id, Arc::clone(&log)));
        let kernel = build_kernel(ao);
        kernel.start();

        let event = Arc::new(TimeEvent::new(ao_id, TimeEventConfig::new(Signal(21))));
        event.set_trace(kernel.trace_hook());

        let mut wheel = QkTimerWheel::new(Arc::clone(&kernel));
        wheel.register(Arc::clone(&event));

        event.arm(1, None);

        wheel.tick().expect("tick should succeed");

        let entries = log.lock().unwrap();
        assert_eq!(entries.as_slice(), &[(ao_id, Signal(21))]);
    }

    #[test]
    fn periodic_time_event_rearms() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(11);
        let ao = new_active_object(ao_id, 5, Recorder::new(ao_id, Arc::clone(&log)));
        let kernel = build_kernel(ao);
        kernel.start();

        let event = Arc::new(TimeEvent::new(ao_id, TimeEventConfig::new(Signal(22))));
        event.set_trace(kernel.trace_hook());
        event.set_trace_meta(TimeEventTraceInfo {
            time_event_addr: 0xAA,
            target_addr: 0xBB,
            tick_rate: 0,
        });

        let mut wheel = QkTimerWheel::new(Arc::clone(&kernel));
        wheel.register(Arc::clone(&event));

        event.arm(1, Some(2));

        for _ in 0..5 {
            wheel.tick().expect("tick should succeed");
        }

        let entries = log.lock().unwrap();
        assert!(entries.len() >= 2);
        assert!(entries
            .iter()
            .all(|(id, sig)| *id == ao_id && *sig == Signal(22)));
    }

    #[test]
    fn multi_tick_rate_domains() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let ao_id = ActiveObjectId::new(12);
        let ao = new_active_object(ao_id, 6, Recorder::new(ao_id, Arc::clone(&log)));
        let kernel = build_kernel(ao);
        kernel.start();

        // Rate 0 event
        let event0 = Arc::new(TimeEvent::new(ao_id, TimeEventConfig::new(Signal(25)).with_tick_rate(0)));
        event0.arm(1, None);

        // Rate 1 event
        let event1 = Arc::new(TimeEvent::new(ao_id, TimeEventConfig::new(Signal(26)).with_tick_rate(1)));
        event1.arm(1, None);

        let mut wheel = QkTimerWheel::new(Arc::clone(&kernel));
        wheel.register(Arc::clone(&event0));
        wheel.register(Arc::clone(&event1));

        assert!(!wheel.no_active(0));
        assert!(!wheel.no_active(1));
        assert!(wheel.no_active(2));

        // Tick rate 0
        wheel.tick_rate(0).expect("tick 0 should succeed");
        {
            let entries = log.lock().unwrap();
            assert_eq!(entries.as_slice(), &[(ao_id, Signal(25))]);
        }

        assert!(wheel.no_active(0));
        assert!(!wheel.no_active(1));

        // Tick rate 1
        wheel.tick_rate(1).expect("tick 1 should succeed");
        {
            let entries = log.lock().unwrap();
            assert_eq!(entries.as_slice(), &[(ao_id, Signal(25)), (ao_id, Signal(26))]);
        }

        assert!(wheel.no_active(1));
    }
}
