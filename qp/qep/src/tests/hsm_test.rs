//! State machine tests for qp-qep

use crate::QHsm;
use qp_core::{QEvent, QSignal, QStateReturn, QStateMachine};

// Simple test event
#[derive(Debug, Clone, Copy)]
struct TestEvent(QSignal);

impl QEvent for TestEvent {
    fn signal(&self) -> QSignal {
        self.0
    }
}

// Test state handlers
fn top_state(_hsm: &mut dyn QStateMachine, event: &dyn QEvent) -> QStateReturn {
    match event.signal() {
        QSignal::INIT => QStateReturn::Initial(state_a),
        _ => QStateReturn::Handled,
    }
}

fn state_a(_hsm: &mut dyn QStateMachine, event: &dyn QEvent) -> QStateReturn {
    match event.signal() {
        s if s == QSignal::new(10) => QStateReturn::Transition(state_b),
        _ => QStateReturn::Super(top_state),
    }
}

fn state_b(_hsm: &mut dyn QStateMachine, event: &dyn QEvent) -> QStateReturn {
    match event.signal() {
        s if s == QSignal::new(20) => QStateReturn::Transition(state_a),
        _ => QStateReturn::Super(top_state),
    }
}

#[test]
fn test_hsm_creation() {
    let hsm = QHsm::new(top_state);
    assert_eq!(hsm.state() as usize, top_state as usize);
}

#[test]
fn test_hsm_init() {
    let mut hsm = QHsm::new(top_state);
    let result = hsm.init();
    assert!(result.is_ok());
    // After init, should be in state_a
    assert_eq!(hsm.state() as usize, state_a as usize);
}

#[test]
fn test_hsm_dispatch() {
    let mut hsm = QHsm::new(top_state);
    hsm.init().unwrap();
    
    // Dispatch event that triggers transition from state_a to state_b
    let evt = TestEvent(QSignal::new(10));
    let result = hsm.dispatch(&evt);
    assert!(result.is_ok());
    assert_eq!(hsm.state() as usize, state_b as usize);
}

#[test]
fn test_hsm_bidirectional_transitions() {
    let mut hsm = QHsm::new(top_state);
    hsm.init().unwrap();
    
    // a -> b
    let evt1 = TestEvent(QSignal::new(10));
    hsm.dispatch(&evt1).unwrap();
    assert_eq!(hsm.state() as usize, state_b as usize);
    
    // b -> a
    let evt2 = TestEvent(QSignal::new(20));
    hsm.dispatch(&evt2).unwrap();
    assert_eq!(hsm.state() as usize, state_a as usize);
}
