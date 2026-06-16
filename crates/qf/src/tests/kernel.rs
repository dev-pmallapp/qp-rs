use std::sync::{Arc, Mutex};

use crate::active::{new_active_object, ActiveContext, ActiveObject, ActiveRunnable, SignalHandler};
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
fn queue_high_watermark_is_sticky() {
    let ao = ActiveObject::new(ActiveObjectId::new(9), 1, Collector::default());
    assert_eq!(ao.queue_len(), 0);
    assert_eq!(ao.queue_high_watermark(), 0);

    ActiveRunnable::post(&*ao, DynEvent::empty_dyn(Signal(1)));
    ActiveRunnable::post(&*ao, DynEvent::empty_dyn(Signal(2)));
    assert_eq!(ao.queue_len(), 2);
    assert_eq!(ao.queue_high_watermark(), 2);

    // Draining the queue must not lower the high-water mark.
    assert!(ActiveRunnable::dispatch_one(&*ao));
    assert_eq!(ao.queue_len(), 1);
    assert_eq!(ao.queue_high_watermark(), 2);
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
