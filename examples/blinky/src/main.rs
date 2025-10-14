//! Blinky Example - A simple LED blinking application using QP framework
//!
//! This example demonstrates basic QP framework usage

#![no_std]
#![no_main]

use qp_core::{QSignal, QEvent, QPriority};
use panic_halt as _; // Panic handler for embedded

// Define signals for our application
const TIMEOUT_SIG: QSignal = QSignal::new(10);
const BUTTON_SIG: QSignal = QSignal::new(11);

// Simple timeout event
#[derive(Debug, Clone, Copy)]
struct TimeoutEvent;

impl QEvent for TimeoutEvent {
    fn signal(&self) -> QSignal {
        TIMEOUT_SIG
    }
}

// Simple button event
#[derive(Debug, Clone, Copy)]
struct ButtonEvent;

impl QEvent for ButtonEvent {
    fn signal(&self) -> QSignal {
        BUTTON_SIG
    }
}

#[no_mangle]
pub extern "C" fn main(_argc: isize, _argv: *const *const u8) -> isize {
    // Simple demo of QP types
    let timeout_evt = TimeoutEvent;
    let button_evt = ButtonEvent;
    
    // Check that our events have the right signals
    assert_eq!(timeout_evt.signal(), TIMEOUT_SIG);
    assert_eq!(button_evt.signal(), BUTTON_SIG);
    
    // Test priority types
    let high_prio = QPriority::new(1).unwrap();
    let low_prio = QPriority::new(10).unwrap();
    
    assert!(high_prio < low_prio);
    
    // Success
    0
}