//! Unit tests for ISR nesting, run/stop lifecycle, rearm/was_disarmed,
//! post_from_isr, and tick_from_isr.

// Under `static-alloc`, the kernel / time-event handles are `Copy` `&'static`
// references, so `.clone()` on them is a no-op (it bumps an `Arc` refcount on
// the dynamic build). Suppress the lint only on the heap-free build.
#![cfg_attr(feature = "static-alloc", allow(noop_method_call))]

use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

use crate::active::{new_active_object, ActiveContext, ActiveObjectId, SignalHandler};
use crate::event::Signal;
use crate::isr;
use crate::kernel::Kernel;
use crate::time::{new_time_event, share_kernel, TimeEventConfig, TimerWheel};

// ── ISR Test Synchronization ──────────────────────────────────────────────────
static ISR_TEST_MUTEX: Mutex<()> = Mutex::new(());

fn lock_isr_test() -> std::sync::MutexGuard<'static, ()> {
    let guard = ISR_TEST_MUTEX.lock().unwrap();
    // Reset global nesting counter to ensure a clean starting point.
    isr::ISR_NESTING.store(0, Ordering::SeqCst);
    guard
}

// ── ISR nesting counter ───────────────────────────────────────────────────────

#[test]
fn isr_nesting_starts_at_zero() {
    let _guard = lock_isr_test();
    assert_eq!(isr::isr_nesting(), 0);
    assert!(!isr::in_isr());
}

#[test]
fn qk_isr_entry_exit_macros_track_nesting() {
    let _guard = lock_isr_test();
    assert_eq!(isr::isr_nesting(), 0);

    crate::qk_isr_entry!();
    assert_eq!(isr::isr_nesting(), 1);
    assert!(isr::in_isr());

    crate::qk_isr_entry!(); // nested ISR
    assert_eq!(isr::isr_nesting(), 2);

    crate::qk_isr_exit!();
    assert_eq!(isr::isr_nesting(), 1);

    crate::qk_isr_exit!();
    assert_eq!(isr::isr_nesting(), 0);
    assert!(!isr::in_isr());
}

// ── Kernel run / stop lifecycle ───────────────────────────────────────────────

struct Counter(Arc<Mutex<usize>>);
impl SignalHandler for Counter {
    fn handle_signal(&mut self, _: Signal, _: &mut ActiveContext) {
        *self.0.lock().unwrap() += 1;
    }
}

#[test]
fn kernel_run_processes_events_and_stop_exits_loop() {
    let count = Arc::new(Mutex::new(0usize));
    let ao_id = ActiveObjectId::new(1);
    let ao = new_active_object(ao_id, 1, Counter(count.clone()));
    let kernel = share_kernel(Kernel::builder().register(ao).build());

    // Post 3 events before starting run.
    kernel.post(ao_id, crate::event::DynEvent::empty_dyn(Signal(1))).unwrap();
    kernel.post(ao_id, crate::event::DynEvent::empty_dyn(Signal(2))).unwrap();
    kernel.post(ao_id, crate::event::DynEvent::empty_dyn(Signal(3))).unwrap();

    // run() with a tick_fn that stops the kernel immediately.
    // The loop still drains all pending events before checking stop_flag.
    let k = kernel.clone();
    kernel.run(move || {
        k.stop();
    });

    assert_eq!(*count.lock().unwrap(), 3, "all events processed");
}

#[test]
fn has_pending_work_reflects_queue_state() {
    let ao_id = ActiveObjectId::new(2);
    let ao = new_active_object(ao_id, 1, Counter(Arc::new(Mutex::new(0))));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    assert!(!kernel.has_pending_work());
    kernel.post(ao_id, crate::event::DynEvent::empty_dyn(Signal(1))).unwrap();
    assert!(kernel.has_pending_work());
    kernel.run_until_idle();
    assert!(!kernel.has_pending_work());
}

// ── post_from_isr / publish_from_isr ─────────────────────────────────────────

#[test]
fn post_from_isr_delivers_event() {
    let _guard = lock_isr_test();
    let count = Arc::new(Mutex::new(0usize));
    let ao_id = ActiveObjectId::new(3);
    let ao = new_active_object(ao_id, 1, Counter(count.clone()));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    crate::qk_isr_entry!();
    kernel.post_from_isr(ao_id, crate::event::DynEvent::empty_dyn(Signal(5))).unwrap();
    crate::qk_isr_exit!();

    kernel.run_until_idle();
    assert_eq!(*count.lock().unwrap(), 1);
}

#[test]
fn publish_from_isr_broadcasts_to_all_aos() {
    let _guard = lock_isr_test();
    let count_a = Arc::new(Mutex::new(0usize));
    let count_b = Arc::new(Mutex::new(0usize));
    let ao_a = new_active_object(ActiveObjectId::new(4), 1, Counter(count_a.clone()));
    let ao_b = new_active_object(ActiveObjectId::new(5), 2, Counter(count_b.clone()));
    let kernel = Arc::new(
        Kernel::builder().register(ao_a).register(ao_b).build()
    );
    kernel.start();

    crate::qk_isr_entry!();
    kernel.publish_from_isr(Signal(7), crate::event::DynEvent::empty_dyn(Signal(7)));
    crate::qk_isr_exit!();

    kernel.run_until_idle();
    assert_eq!(*count_a.lock().unwrap(), 1);
    assert_eq!(*count_b.lock().unwrap(), 1);
}

// ── TimeEvent::rearm / was_disarmed ──────────────────────────────────────────

#[test]
fn rearm_updates_counter_without_disarm_cycle() {
    let ao_id = ActiveObjectId::new(6);
    let ao = new_active_object(ao_id, 1, Counter(Arc::new(Mutex::new(0))));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    let te = new_time_event(ao_id, TimeEventConfig::new(Signal(0x20)));
    te.arm(5, None);
    assert!(te.is_armed());

    // rearm with 3 ticks: counter updates, still armed, returns true (was armed)
    let was_armed = te.rearm(3);
    assert!(was_armed);
    assert!(te.is_armed());

    // rearm on a disarmed event: arms it, returns false (was not armed)
    te.disarm();
    let was_armed2 = te.rearm(10);
    assert!(!was_armed2);
    assert!(te.is_armed());
}

#[test]
fn was_disarmed_set_on_explicit_disarm() {
    let ao_id = ActiveObjectId::new(7);
    let ao = new_active_object(ao_id, 1, Counter(Arc::new(Mutex::new(0))));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    let te = new_time_event(ao_id, TimeEventConfig::new(Signal(0x21)));
    te.arm(5, None);
    assert!(!te.was_disarmed(), "not yet disarmed");
    te.disarm();
    assert!(te.was_disarmed(), "flag set after disarm");
    assert!(!te.was_disarmed(), "flag cleared after read");
}

#[test]
fn was_disarmed_set_when_oneshot_fires() {
    let count = Arc::new(Mutex::new(0usize));
    let ao_id = ActiveObjectId::new(8);
    let ao = new_active_object(ao_id, 1, Counter(count.clone()));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    let te = new_time_event(ao_id, TimeEventConfig::new(Signal(0x22)));
    let mut wheel = TimerWheel::new(kernel.clone());
    wheel.register(te.clone());

    te.arm(1, None); // one-shot: fires after 1 tick
    wheel.tick().unwrap();
    kernel.run_until_idle();

    assert_eq!(*count.lock().unwrap(), 1, "event fired");
    assert!(te.was_disarmed(), "was_disarmed flag set after auto-disarm");
    assert!(!te.was_disarmed(), "flag cleared after read");
}

// ── tick_from_isr ─────────────────────────────────────────────────────────────

#[test]
fn tick_from_isr_advances_timer_wheel() {
    let _guard = lock_isr_test();
    let count = Arc::new(Mutex::new(0usize));
    let ao_id = ActiveObjectId::new(9);
    let ao = new_active_object(ao_id, 1, Counter(count.clone()));
    let kernel = share_kernel(Kernel::builder().register(ao).build());
    kernel.start();

    let te = new_time_event(ao_id, TimeEventConfig::new(Signal(0x30)));
    let mut wheel = TimerWheel::new(kernel.clone());
    wheel.register(te.clone());

    te.arm(1, None);

    crate::qk_isr_entry!();
    wheel.tick_from_isr().unwrap();
    crate::qk_isr_exit!();

    kernel.run_until_idle();
    assert_eq!(*count.lock().unwrap(), 1, "tick_from_isr delivered event");
}
