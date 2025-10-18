use std::sync::{Arc, Mutex};

use crate::active::{new_active_object, ActiveContext, SignalHandler};
use crate::event::{DynEvent, Signal};
use crate::kernel::Kernel;
use crate::ActiveObjectId;

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
fn kernel_delivers_events() {
    let collector = Collector::default();
    let probe = collector.clone();

    let ao = new_active_object(ActiveObjectId::new(1), 1, collector);
    let kernel = Kernel::builder().register(ao).build();
    kernel.start();

    kernel
        .post(ActiveObjectId::new(1), DynEvent::empty_dyn(Signal(0x42)))
        .unwrap();
    kernel.run_until_idle();

    let events = probe.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], Signal(0x42));
}
