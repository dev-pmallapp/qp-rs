//! QV Scheduler Integration for POSIX
//!
//! Provides the main event loop integration for running QP applications on POSIX systems.

use std::sync::atomic::{AtomicBool, Ordering};

/// Global running flag
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Run the QV scheduler
///
/// This is the main entry point for QP applications on POSIX. It starts the
/// ticker thread and enters the event processing loop.
///
/// # Examples
///
/// ```no_run
/// use qp_posix;
///
/// fn main() {
///     qp_posix::init();
///     
///     // Initialize active objects here
///     
///     qp_posix::run();
/// }
/// ```
pub fn run() -> ! {
    println!("Starting QP POSIX application...");
    
    // Start the ticker
    crate::time::start_ticker();
    
    RUNNING.store(true, Ordering::SeqCst);
    
    // Application startup callback
    on_startup();
    
    // Main event loop
    while RUNNING.load(Ordering::Relaxed) {
        // Process events from all active objects
        // This will be integrated with QV scheduler
        
        // For now, just sleep to avoid busy-wait
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    // Cleanup
    on_cleanup();
    crate::cleanup();
    
    std::process::exit(0);
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
/// Override this in your application to perform cleanup before exit.
#[allow(unused)]
pub fn on_cleanup() {
    // Default implementation - can be overridden by application
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop() {
        RUNNING.store(true, Ordering::SeqCst);
        stop();
        assert!(!RUNNING.load(Ordering::SeqCst));
    }
}
