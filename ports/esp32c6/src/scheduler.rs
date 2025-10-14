//! Scheduler Integration for ESP32-C6 Port
//!
//! Connects QP scheduler with ESP32-C6 board event loop.

use core::sync::atomic::{AtomicBool, Ordering};

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Run the QP scheduler
///
/// Enters the main event loop and never returns.
pub fn run() -> ! {
    RUNNING.store(true, Ordering::SeqCst);
    
    // Application startup callback
    on_startup();
    
    // Main event loop
    loop {
        // TODO: Process events from active object queues
        // TODO: Call board::idle() when no events pending
        
        if !RUNNING.load(Ordering::Relaxed) {
            break;
        }
    }
    
    on_cleanup();
    
    // In embedded, we typically just halt or reset
    loop {
        // TODO: board::wait_for_interrupt();
    }
}

/// Stop the scheduler
pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
}

/// Application startup callback
///
/// Override this in your application to perform startup initialization.
#[allow(unused)]
pub fn on_startup() {
    // Default implementation - can be overridden by application
}

/// Application cleanup callback
///
/// Override this in your application to perform cleanup before halt.
#[allow(unused)]
pub fn on_cleanup() {
    // Default implementation - can be overridden by application
}
