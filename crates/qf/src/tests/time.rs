use std::sync::{Arc, Mutex};

use crate::active::{new_active_object, ActiveContext, SignalHandler};
use crate::kernel::Kernel;
use crate::time::{TimeEvent, TimeEventConfig, TimerWheel};
use crate::{ActiveObjectId, Signal};

#[derive(Clone, Default)]
struct Collector {
    events: Arc<Mutex<Vec<Signal>>>,
}

impl SignalHandler for Collector {
    fn on_start(&mut self, _ctx: &mut ActiveContext) {}

    fn handle_signal(&mut self, signal: Signal, _ctx: &mut ActiveContext) {
        self.events.lock().unwrap().push(signal);
    }
}

#[test]
fn time_event_fires_after_tick() {
    let collector = Collector::default();
    let probe = collector.clone();
    let ao = new_active_object(ActiveObjectId::new(1), 1, collector);
    let kernel = Arc::new(Kernel::builder().register(ao).build());
    kernel.start();

    let mut wheel = TimerWheel::new(kernel.clone());
    let time_evt = TimeEvent::new(ActiveObjectId::new(1), TimeEventConfig::new(Signal(0x10)));
    time_evt.arm(1, None);
    wheel.register(time_evt);

    wheel.tick().unwrap();
    kernel.run_until_idle();

    let events = probe.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Signal(0x10));
}
