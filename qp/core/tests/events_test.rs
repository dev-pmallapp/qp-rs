//! Event tests for qp-core
//! These tests run on x86 host with std for testing, but verify no_std compatible code

use qp_core::{QEvent, QSignal};

// Define test event types
#[derive(Debug, Clone, Copy)]
struct TestEvent {
    signal: QSignal,
}

impl QEvent for TestEvent {
    fn signal(&self) -> QSignal {
        self.signal
    }
}

#[test]
fn test_event_signal() {
    let sig = QSignal::new(10);
    let event = TestEvent { signal: sig };
    assert_eq!(event.signal(), sig);
}

#[test]
fn test_signal_creation() {
    let sig1 = QSignal::new(1);
    let sig2 = QSignal::new(2);
    assert_ne!(sig1, sig2);
}

#[test]
fn test_signal_equality() {
    let sig1 = QSignal::new(42);
    let sig2 = QSignal::new(42);
    assert_eq!(sig1, sig2);
}
