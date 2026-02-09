use alloc::vec::Vec;
use core::fmt;

use qf::time::TimeEvent;
use qf::TraceHook;

use crate::kernel::{QkKernel, QkKernelError};
use crate::sync::Arc;

pub struct QkTimerWheel {
    kernel: Arc<QkKernel>,
    events: Vec<Arc<TimeEvent>>,
    trace: Option<TraceHook>,
}

impl QkTimerWheel {
    pub fn new(kernel: Arc<QkKernel>) -> Self {
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

    pub fn tick(&self) -> Result<(), QkTimeEventError> {
        for event in &self.events {
            if let Some((target, evt)) = event.poll() {
                self.kernel.post_and_run(target, evt)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum QkTimeEventError {
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
}
