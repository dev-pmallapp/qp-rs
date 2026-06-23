//! Unit tests for QEQueue and defer/recall.

use crate::active::{new_active_object, ActiveBehavior, ActiveContext, ActiveObjectId};
use crate::equeue::{defer, flush_deferred, recall, QEQueue};
use crate::event::{DynEvent, Signal};

fn ev(sig: u16) -> DynEvent {
    DynEvent::empty_dyn(Signal(sig))
}

// ── QEQueue basic operations ──────────────────────────────────────────────────

#[test]
fn equeue_new_is_empty() {
    let q = QEQueue::new(4);
    assert!(q.is_empty());
    assert_eq!(q.len(), 0);
    assert_eq!(q.capacity(), 4);
    assert_eq!(q.get_free(), 4);
    assert_eq!(q.get_min(), 4);
}

#[test]
fn equeue_post_and_get_fifo() {
    let q = QEQueue::new(4);
    assert!(q.post(ev(1), 0));
    assert!(q.post(ev(2), 0));
    assert_eq!(q.len(), 2);

    let first = q.get().expect("should have event");
    assert_eq!(first.signal(), Signal(1));
    let second = q.get().expect("should have event");
    assert_eq!(second.signal(), Signal(2));
    assert!(q.get().is_none());
}

#[test]
fn equeue_post_lifo_prepends() {
    let q = QEQueue::new(4);
    q.post(ev(1), 0);
    q.post(ev(2), 0);
    // LIFO insert: goes to front
    q.post_lifo(ev(99));
    assert_eq!(q.get().unwrap().signal(), Signal(99));
    assert_eq!(q.get().unwrap().signal(), Signal(1));
    assert_eq!(q.get().unwrap().signal(), Signal(2));
}

#[test]
fn equeue_capacity_enforced() {
    let q = QEQueue::new(2);
    assert!(q.post(ev(1), 0));
    assert!(q.post(ev(2), 0));
    // Full: margin=0 still fails when capacity reached
    assert!(!q.post(ev(3), 0));
    assert!(!q.post_lifo(ev(3)));
    assert_eq!(q.len(), 2);
}

#[test]
fn equeue_margin_leaves_slots() {
    let q = QEQueue::new(4);
    q.post(ev(1), 0);
    q.post(ev(2), 0);
    q.post(ev(3), 0);
    // 1 slot free; margin=1 means "keep ≥1 free" → fails
    assert!(!q.post(ev(4), 1));
    // margin=0 succeeds
    assert!(q.post(ev(4), 0));
}

#[test]
fn equeue_get_free_and_len() {
    let q = QEQueue::new(3);
    assert_eq!(q.get_free(), 3);
    q.post(ev(1), 0);
    assert_eq!(q.get_free(), 2);
    assert_eq!(q.len(), 1);
    q.get();
    assert_eq!(q.get_free(), 3);
    assert_eq!(q.len(), 0);
}

#[test]
fn equeue_watermark_sticky() {
    let q = QEQueue::new(4);
    q.post(ev(1), 0);
    q.post(ev(2), 0);
    q.post(ev(3), 0);
    assert_eq!(q.get_min(), 1); // 1 slot was free at minimum
    // Drain completely
    while q.get().is_some() {}
    // Watermark stays at 1 (never resets upward)
    assert_eq!(q.get_min(), 1);
    // Fill all 4 slots
    q.post(ev(1), 0);
    q.post(ev(2), 0);
    q.post(ev(3), 0);
    q.post(ev(4), 0);
    assert_eq!(q.get_min(), 0);
}

#[test]
fn equeue_peek_front() {
    let q = QEQueue::new(2);
    assert!(!q.peek_front());
    q.post(ev(1), 0);
    assert!(q.peek_front());
    q.get();
    assert!(!q.peek_front());
}

// ── Defer / Recall ────────────────────────────────────────────────────────────

struct Noop;
impl ActiveBehavior for Noop {
    fn on_start(&mut self, _: &mut ActiveContext) {}
    fn on_event(&mut self, _: &mut ActiveContext, _: DynEvent) {}
}

#[test]
fn defer_moves_event_to_eq() {
    let ao = new_active_object(ActiveObjectId(1), 1, Noop);
    let eq = QEQueue::new(4);

    assert!(defer(&*ao, &eq, ev(10)));
    assert!(!eq.is_empty());
    assert!(!ao.has_events(), "deferred event should not be in AO queue");
}

#[test]
fn recall_reinjects_lifo() {
    let ao = new_active_object(ActiveObjectId(1), 1, Noop);
    let eq = QEQueue::new(4);

    // Post a "normal" event to the AO first.
    ao.post(ev(1));
    // Defer a second event.
    defer(&*ao, &eq, ev(2));
    // Recall: ev(2) should go to the FRONT of AO's queue.
    assert!(recall(&*ao, &eq));
    assert!(eq.is_empty());

    // Dispatch: ev(2) should be processed before ev(1).
    struct Capture(alloc::sync::Arc<crate::sync::Mutex<alloc::vec::Vec<u16>>>);
    impl ActiveBehavior for Capture {
        fn on_start(&mut self, _: &mut ActiveContext) {}
        fn on_event(&mut self, _: &mut ActiveContext, e: DynEvent) {
            self.0.lock().push(e.signal().0);
        }
    }
    let log = alloc::sync::Arc::new(crate::sync::Mutex::new(alloc::vec::Vec::new()));
    let ao2 = new_active_object(ActiveObjectId(2), 2, Capture(log.clone()));
    ao2.post(ev(1));
    ao2.post_lifo(ev(2)); // simulate what recall does
    ao2.dispatch_one();
    ao2.dispatch_one();
    let order = log.lock().clone();
    assert_eq!(order, alloc::vec![2u16, 1u16], "recalled event dispatched first");
}

#[test]
fn recall_returns_false_when_eq_empty() {
    let ao = new_active_object(ActiveObjectId(1), 1, Noop);
    let eq = QEQueue::new(4);
    assert!(!recall(&*ao, &eq));
}

#[test]
fn flush_deferred_discards_all() {
    let eq = QEQueue::new(4);
    eq.post(ev(1), 0);
    eq.post(ev(2), 0);
    eq.post(ev(3), 0);
    let n = flush_deferred(&eq, usize::MAX);
    assert_eq!(n, 3);
    assert!(eq.is_empty());
}

#[test]
fn flush_deferred_respects_limit() {
    let eq = QEQueue::new(4);
    eq.post(ev(1), 0);
    eq.post(ev(2), 0);
    eq.post(ev(3), 0);
    let n = flush_deferred(&eq, 2);
    assert_eq!(n, 2);
    assert_eq!(eq.len(), 1);
}

#[test]
fn defer_full_queue_returns_false() {
    let ao = new_active_object(ActiveObjectId(1), 1, Noop);
    let eq = QEQueue::new(2);
    assert!(defer(&*ao, &eq, ev(1)));
    assert!(defer(&*ao, &eq, ev(2)));
    assert!(!defer(&*ao, &eq, ev(3)), "full queue should reject");
}
