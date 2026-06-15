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
fn pubsub_delivers_only_to_subscribers() {
    let coll1 = Collector::default();
    let probe1 = coll1.clone();
    let ao1 = new_active_object(ActiveObjectId::new(1), 1, coll1);

    let coll2 = Collector::default();
    let probe2 = coll2.clone();
    let ao2 = new_active_object(ActiveObjectId::new(2), 2, coll2);

    let kernel = Kernel::builder()
        .ps_init(100) // max_signal = 100
        .register(ao1)
        .register(ao2)
        .build();

    kernel.start();

    // Subscribe AO1 to Signal(10)
    kernel.subscribe(Signal(10), 1); // AO1 priority is 1

    // Subscribe AO2 to Signal(20)
    kernel.subscribe(Signal(20), 2); // AO2 priority is 2

    // Publish Signal(10)
    kernel.publish(Signal(10), DynEvent::empty_dyn(Signal(10)));
    kernel.run_until_idle();

    // Only AO1 should receive it
    assert_eq!(probe1.events.lock().unwrap().as_slice(), &[Signal(10)]);
    assert!(probe2.events.lock().unwrap().is_empty());

    // Publish Signal(20)
    kernel.publish(Signal(20), DynEvent::empty_dyn(Signal(20)));
    kernel.run_until_idle();

    assert_eq!(probe1.events.lock().unwrap().as_slice(), &[Signal(10)]);
    assert_eq!(probe2.events.lock().unwrap().as_slice(), &[Signal(20)]);

    // Unsubscribe AO1 from Signal(10)
    kernel.unsubscribe(Signal(10), 1);

    // Publish Signal(10) again
    kernel.publish(Signal(10), DynEvent::empty_dyn(Signal(10)));
    kernel.run_until_idle();

    // AO1 should NOT receive it this time
    assert_eq!(probe1.events.lock().unwrap().as_slice(), &[Signal(10)]);
}

#[test]
fn pubsub_unsubscribe_all() {
    let coll = Collector::default();
    let probe = coll.clone();
    let ao = new_active_object(ActiveObjectId::new(1), 1, coll);

    let kernel = Kernel::builder()
        .ps_init(100)
        .register(ao)
        .build();

    kernel.start();

    kernel.subscribe(Signal(10), 1);
    kernel.subscribe(Signal(20), 1);

    kernel.publish(Signal(10), DynEvent::empty_dyn(Signal(10)));
    kernel.publish(Signal(20), DynEvent::empty_dyn(Signal(20)));
    kernel.run_until_idle();

    assert_eq!(probe.events.lock().unwrap().as_slice(), &[Signal(10), Signal(20)]);

    // Unsubscribe from all
    kernel.unsubscribe_all(1);

    kernel.publish(Signal(10), DynEvent::empty_dyn(Signal(10)));
    kernel.publish(Signal(20), DynEvent::empty_dyn(Signal(20)));
    kernel.run_until_idle();

    // No new events should be received
    assert_eq!(probe.events.lock().unwrap().as_slice(), &[Signal(10), Signal(20)]);
}

#[test]
fn fallback_broadcast_when_no_ps_init() {
    let coll1 = Collector::default();
    let probe1 = coll1.clone();
    let ao1 = new_active_object(ActiveObjectId::new(1), 1, coll1);

    let coll2 = Collector::default();
    let probe2 = coll2.clone();
    let ao2 = new_active_object(ActiveObjectId::new(2), 2, coll2);

    // No ps_init
    let kernel = Kernel::builder()
        .register(ao1)
        .register(ao2)
        .build();

    kernel.start();

    // Publish Signal(10)
    kernel.publish(Signal(10), DynEvent::empty_dyn(Signal(10)));
    kernel.run_until_idle();

    // Both should receive it unconditionally (backward-compatibility)
    assert_eq!(probe1.events.lock().unwrap().as_slice(), &[Signal(10)]);
    assert_eq!(probe2.events.lock().unwrap().as_slice(), &[Signal(10)]);
}
