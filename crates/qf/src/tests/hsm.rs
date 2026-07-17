//! Unit tests for the QHsm hierarchical state machine framework.
//!
//! Test topology (same shape as the QP/C++ QHsm example):
//!
//! ```text
//! top
//! └── s                  (composite)
//!     ├── s1             (composite)
//!     │   ├── s11        (leaf)
//!     │   └── (initial → s11)
//!     └── s2             (composite)
//!         ├── s21        (leaf)
//!         └── (initial → s21)
//! initial pseudo-state → s2 → s21
//! ```
//!
//! Signals used:
//! - A (4): s1  → s1  (self-transition)
//! - B (5): s1  → s11 (internal tran with guard check)
//! - C (6): s1  → s2  (transition sibling composite)
//! - D (7): s11 → s   (transition up two levels)
//! - E (8): s   → s11 (transition into composite sub-state)
//! - F (9): s2  → s1  (transition sibling)
//! - G (10): s21 → s1 (transition up)
//! - H (11): s1 → history of s (TranHist)
//! - I (12): internal tran at s (should NOT change state)

use std::sync::{Arc, Mutex};

use crate::active::{ActiveObjectId};
use crate::event::{DynEvent, Signal};
#[allow(deprecated)]
use crate::hsm::same_state;
use crate::hsm::{SameState, QHsm, QHsmResult};
use crate::hsm::reserved::*;
use crate::kernel::Kernel;
use crate::{q_handled, q_ignored, q_super, q_tran, q_tran_hist};

// ── Signal definitions ────────────────────────────────────────────────────────
const A_SIG: u16 = 4;
const B_SIG: u16 = 5;
const C_SIG: u16 = 6;
const D_SIG: u16 = 7;
const E_SIG: u16 = 8;
const F_SIG: u16 = 9;
const G_SIG: u16 = 10;
const H_SIG: u16 = 11;
const I_SIG: u16 = 12;

// ── State machine data ────────────────────────────────────────────────────────

#[derive(Default)]
struct TestSm {
    /// Log of all entry/exit/init/event actions, in order.
    trace: Vec<&'static str>,
    /// Counts how many times state `s` internal I-signal was handled.
    i_handled: u32,
    /// Auxiliary flag toggled by B_SIG guard.
    foo: bool,
}

// ── State handlers ────────────────────────────────────────────────────────────

fn initial(sm: &mut TestSm, _e: &DynEvent) -> QHsmResult<TestSm> {
    sm.trace.push("initial");
    q_tran!(s2)
}

fn s(sm: &mut TestSm, e: &DynEvent) -> QHsmResult<TestSm> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.trace.push("s-ENTRY"); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.trace.push("s-EXIT");  q_handled!() }
        Q_INIT_SIG_VAL  => { sm.trace.push("s-INIT");  q_tran!(s11) }
        E_SIG => { sm.trace.push("s-E"); q_tran!(s11) }
        I_SIG => { sm.i_handled += 1; q_handled!() }
        _ => q_super!(QHsm::<TestSm>::top_state),
    }
}

fn s1(sm: &mut TestSm, e: &DynEvent) -> QHsmResult<TestSm> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.trace.push("s1-ENTRY"); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.trace.push("s1-EXIT");  q_handled!() }
        Q_INIT_SIG_VAL  => { sm.trace.push("s1-INIT");  q_tran!(s11) }
        A_SIG => { sm.trace.push("s1-A"); q_tran!(s1) }   // self-tran on s1
        B_SIG => {
            // Guard: only transition if foo is false.
            if !sm.foo {
                sm.trace.push("s1-B");
                sm.foo = true;
                q_tran!(s11)
            } else {
                q_ignored!()
            }
        }
        C_SIG => { sm.trace.push("s1-C"); q_tran!(s2) }
        D_SIG => { sm.trace.push("s1-D"); q_tran!(s) }
        F_SIG => { sm.trace.push("s1-F"); q_tran!(s2) }
        H_SIG => { sm.trace.push("s1-H"); q_tran_hist!(s) }
        _ => q_super!(s),
    }
}

fn s11(sm: &mut TestSm, e: &DynEvent) -> QHsmResult<TestSm> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.trace.push("s11-ENTRY"); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.trace.push("s11-EXIT");  q_handled!() }
        G_SIG => { sm.trace.push("s11-G"); q_tran!(s1) }
        _ => q_super!(s1),
    }
}

fn s2(sm: &mut TestSm, e: &DynEvent) -> QHsmResult<TestSm> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.trace.push("s2-ENTRY"); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.trace.push("s2-EXIT");  q_handled!() }
        Q_INIT_SIG_VAL  => { sm.trace.push("s2-INIT");  q_tran!(s21) }
        C_SIG => { sm.trace.push("s2-C"); q_tran!(s1) }
        F_SIG => { sm.trace.push("s2-F"); q_tran!(s1) }
        _ => q_super!(s),
    }
}

fn s21(sm: &mut TestSm, e: &DynEvent) -> QHsmResult<TestSm> {
    match e.signal().0 {
        Q_ENTRY_SIG_VAL => { sm.trace.push("s21-ENTRY"); q_handled!() }
        Q_EXIT_SIG_VAL  => { sm.trace.push("s21-EXIT");  q_handled!() }
        _ => q_super!(s2),
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_hsm() -> QHsm<TestSm> {
    QHsm::new(TestSm::default(), initial)
}

fn dispatch(hsm: &mut QHsm<TestSm>, sig: u16) {
    let e = DynEvent::empty_dyn(Signal(sig));
    hsm.dispatch(&e);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
#[allow(deprecated)]
fn state_handler_reports_current_leaf() {
    let mut hsm = make_hsm();
    // Before init the machine is not yet in the leaf state s21.
    assert!(!same_state(hsm.state_handler(), s21));
    hsm.init();
    // After init the active leaf is s21 (initial → s2 → s2-INIT → s21).
    assert!(same_state(hsm.state_handler(), s21));
    assert!(same_state(hsm.state_handler(), s21));
}

#[test]
fn init_enters_hierarchy_top_down() {
    let mut hsm = make_hsm();
    hsm.init();
    // initial → s2, then s2-INIT → s21
    // Entry order: s, s2, s21
    assert_eq!(
        hsm.sm().trace,
        ["initial", "s-ENTRY", "s2-ENTRY", "s2-INIT", "s21-ENTRY"]
    );
    // Current state should be s21 (leaf after nested init).
    // Confirm by dispatching an event only s21's parent (s2) handles.
    dispatch(&mut hsm, C_SIG); // s2-C: s21 → (via s2) → s1
    assert!(hsm.sm().trace.contains(&"s2-C"));
}

#[test]
fn simple_tran_exits_source_enters_target() {
    let mut hsm = make_hsm();
    hsm.init();
    let init_len = hsm.sm().trace.len();

    // s21 is current.  C_SIG on s2 transitions to s1, initial → s11.
    dispatch(&mut hsm, C_SIG);

    let log = &hsm.sm().trace[init_len..];
    // Exit: s21, s2 | Enter: s1, s11 (via s1-INIT)
    assert!(log.contains(&"s21-EXIT"), "should exit s21");
    assert!(log.contains(&"s2-EXIT"),  "should exit s2");
    assert!(log.contains(&"s1-ENTRY"), "should enter s1");
    assert!(log.contains(&"s11-ENTRY"),"should enter s11 via s1-INIT");
    // s should NOT be exited or re-entered (it is LCA of s1 and s2).
    assert!(!log.contains(&"s-EXIT"),  "LCA s must NOT be exited");
    assert!(!log.contains(&"s-ENTRY"), "LCA s must NOT be re-entered");
}

#[test]
fn self_transition_exits_and_reenters() {
    let mut hsm = make_hsm();
    hsm.init();
    // Get into s11: s21 → C → s1 → s11 (initial)
    dispatch(&mut hsm, C_SIG);
    let init_len = hsm.sm().trace.len();

    // A_SIG on s1: self-transition s1 → s1
    dispatch(&mut hsm, A_SIG);

    let log = &hsm.sm().trace[init_len..];
    // exit s11, exit s1, enter s1, then s1-INIT → enter s11
    assert!(log.contains(&"s11-EXIT"), "s11 must exit");
    assert!(log.contains(&"s1-EXIT"),  "s1 must exit (self-tran)");
    assert!(log.contains(&"s1-ENTRY"),"s1 must re-enter");
    assert!(log.contains(&"s11-ENTRY"),"s11 must re-enter via nested init");
    assert!(!log.contains(&"s-EXIT"),  "s must NOT exit on s1 self-tran");
}

#[test]
fn internal_tran_does_not_change_state() {
    let mut hsm = make_hsm();
    hsm.init();
    // currently in s21

    let before_i_handled = hsm.sm().i_handled;
    dispatch(&mut hsm, I_SIG); // handled internally by s (I_SIG)
    assert_eq!(
        hsm.sm().i_handled,
        before_i_handled + 1,
        "I_SIG must be handled by s"
    );
    // no EXIT or ENTRY should occur
    let log = &hsm.sm().trace;
    let entry_exit_after: Vec<&&str> = log
        .iter()
        .filter(|&&s| s.ends_with("-ENTRY") || s.ends_with("-EXIT"))
        .collect::<Vec<_>>();
    // Only the initial entries should be there (from init)
    let init_entries_count = entry_exit_after.len();

    dispatch(&mut hsm, I_SIG);
    let new_count = hsm
        .sm()
        .trace
        .iter()
        .filter(|&&s| s.ends_with("-ENTRY") || s.ends_with("-EXIT"))
        .count();
    assert_eq!(
        new_count, init_entries_count,
        "No new ENTRY/EXIT on internal tran"
    );
}

#[test]
fn tran_up_two_levels_exits_chain() {
    let mut hsm = make_hsm();
    hsm.init();
    // Currently in s21.  Transition to s11 via E_SIG (handled by s → s11).
    dispatch(&mut hsm, E_SIG);
    let log = &hsm.sm().trace;
    // s was LCA? No - E is handled by s and transitions to s11 (substate of s).
    // Exit s21, s2; Enter s11 (s is not exited since it's the source/LCA boundary)
    assert!(log.contains(&"s21-EXIT"), "s21 should exit");
    assert!(log.contains(&"s2-EXIT"),  "s2 should exit");
    assert!(log.contains(&"s11-ENTRY"),"s11 should be entered");
    // s itself should not exit (it's the source and LCA in this tran)
    assert!(!log.contains(&"s-EXIT"),  "s must not exit");
}

#[test]
fn tran_into_composite_runs_nested_init() {
    // Verify nested init in s after D_SIG (s11 → s, then s-INIT → s11).
    let mut hsm = make_hsm();
    hsm.init();
    dispatch(&mut hsm, C_SIG); // → s11 (via s1-INIT)
    let init_len = hsm.sm().trace.len();

    dispatch(&mut hsm, D_SIG); // s1-D: s1 → s
    let log = &hsm.sm().trace[init_len..];
    assert!(log.contains(&"s11-EXIT"), "exit s11");
    assert!(log.contains(&"s1-EXIT"),  "exit s1");
    // s-ENTRY should NOT appear (s is source of tran, so LCA = s's parent = top)
    // Actually: source = s1, target = s, LCA = s (s is in target_path AND source_path)
    // Let's verify the actual behavior:
    // source=s1, target=s → LCA = s (s is ancestor of s1)
    // exit from s11 up to (not incl) s: exit s11, exit s1
    // enter from s (not incl) down to s: nothing (target IS LCA)
    // then handle_nested_init: s-INIT → s11 → enter s11
    assert!(log.contains(&"s-INIT"),   "s-INIT nested init fires");
    assert!(log.contains(&"s11-ENTRY"),"s11 entered via nested init");
}

#[test]
fn tran_hist_restores_last_active_substate() {
    let mut hsm = make_hsm();
    hsm.init();
    // init puts us in s21 (within s2 branch)

    // Go to s11 branch
    dispatch(&mut hsm, C_SIG); // s21 → s11  (via s2-C → s1 → s1-INIT → s11)
    // current = s11, history[s1] = s11

    // Now go back to s21 side via F_SIG
    dispatch(&mut hsm, F_SIG); // s1-F → s2 → s2-INIT → s21
    // current = s21

    // H_SIG on s1 does TranHist(s): transition to last active substate of s
    // But current is s21 — first go back to s1 side.
    dispatch(&mut hsm, C_SIG); // s2-C → s1 → s1-INIT → s11
    // current = s11, history[s] = s2 (was recorded when we exited s2 earlier)

    let init_len = hsm.sm().trace.len();
    // H_SIG: s1-H → TranHist(s)
    // history[s] should be s2 (last child of s that was active)
    dispatch(&mut hsm, H_SIG);
    let log = &hsm.sm().trace[init_len..];
    // Should transition toward s2 branch (the history)
    // The exact landing state depends on whether history carries through s2's init.
    // At minimum, we should see s2-ENTRY or s21-ENTRY.
    let enters_s2_branch = log.contains(&"s2-ENTRY") || log.contains(&"s21-ENTRY");
    assert!(enters_s2_branch, "TranHist should restore s2 branch: {:?}", log);
}

#[test]
fn b_sig_guard_transitions_once_then_ignored() {
    let mut hsm = make_hsm();
    hsm.init();
    dispatch(&mut hsm, C_SIG); // → s11 (current)

    // foo is false → B_SIG should trigger transition to s11 and set foo = true.
    assert!(!hsm.sm().foo);
    let pre_len = hsm.sm().trace.len();
    dispatch(&mut hsm, B_SIG);
    assert!(hsm.sm().foo, "foo should be true after first B_SIG");
    assert!(
        hsm.sm().trace[pre_len..].contains(&"s1-B"),
        "s1-B should fire"
    );

    // Second B_SIG with foo = true → guard fails → ignored.
    let pre_len2 = hsm.sm().trace.len();
    dispatch(&mut hsm, B_SIG);
    assert!(
        !hsm.sm().trace[pre_len2..].contains(&"s1-B"),
        "s1-B must NOT fire when foo is true"
    );
}

#[test]
fn unknown_signal_bubbles_to_top_is_ignored() {
    let mut hsm = make_hsm();
    hsm.init();
    let pre = hsm.sm().trace.len();
    // Signal 99 is not handled by any state.
    dispatch(&mut hsm, 99);
    // No entry/exit should occur.
    let log = &hsm.sm().trace[pre..];
    let no_entry_exit = !log.iter().any(|s| s.ends_with("-ENTRY") || s.ends_with("-EXIT"));
    assert!(no_entry_exit, "unknown signal must not cause transitions: {:?}", log);
}

#[test]
fn hsm_as_active_behavior_via_kernel() {
    // Verify that QHsm<S> can be used as an ActiveObject behavior.
    let log: Arc<Mutex<Vec<Signal>>> = Arc::new(Mutex::new(Vec::new()));
    let log_clone = log.clone();

    // Minimal SM that records every dispatched signal.
    struct Recorder {
        trace: Arc<Mutex<Vec<Signal>>>,
    }

    fn rec_initial(_sm: &mut Recorder, _e: &DynEvent) -> QHsmResult<Recorder> {
        QHsmResult::Tran(rec_active)
    }

    fn rec_active(sm: &mut Recorder, e: &DynEvent) -> QHsmResult<Recorder> {
        match e.signal().0 {
            // Reserved signals (0=EMPTY, 1=ENTRY, 2=EXIT, 3=INIT) — do not log.
            0..=3 => q_handled!(),
            sig => {
                sm.trace.lock().unwrap().push(Signal(sig));
                q_handled!()
            }
        }
    }

    let hsm = QHsm::new(Recorder { trace: log_clone }, rec_initial);
    let ao = crate::active::new_active_object(ActiveObjectId::new(1), 5, hsm);
    let kernel = Kernel::builder().register(ao).build();
    kernel.start();

    kernel
        .post(ActiveObjectId::new(1), DynEvent::empty_dyn(Signal(42)))
        .unwrap();
    kernel.run_until_idle();

    let recorded = log.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0], Signal(42));
}

#[test]
fn g_sig_tran_from_s11_to_s1() {
    // G_SIG on s11 → s1 (s11 exits, s1 re-entered, s11 re-entered via nested init).
    let mut hsm = make_hsm();
    hsm.init();
    dispatch(&mut hsm, C_SIG); // → s11
    let init_len = hsm.sm().trace.len();

    dispatch(&mut hsm, G_SIG); // s11-G: s11 → s1
    let log = &hsm.sm().trace[init_len..];
    assert!(log.contains(&"s11-EXIT"),  "s11 should exit");
    // s1 is ancestor of s11, so s1 is LCA — it does NOT exit.
    // But it IS the target, which means we enter s1 then its nested init → s11.
    // Wait: source = s11, target = s1, LCA = s1.
    // exit from s11 up to (not incl) s1: exit s11 only.
    // enter from s1 (not incl) down to s1: nothing (target == LCA).
    // nested init: s1-INIT → s11 → enter s11.
    assert!(log.contains(&"s1-INIT"),   "nested init in s1");
    assert!(log.contains(&"s11-ENTRY"), "s11 re-entered via nested init");
    assert!(!log.contains(&"s1-EXIT"),  "s1 must not exit (it's the LCA/target)");
}
