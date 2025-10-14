//! QP Framework POSIX Port for Linux/Unix Systems
//!
//! This crate provides the POSIX port of the QP framework, enabling QP applications
//! to run on Linux, macOS, and other POSIX-compliant operating systems.
//!
//! # Features
//!
//! - Critical section management using standard mutexes
//! - Clock tick service using high-resolution timers
//! - Thread-based active object execution
//! - Integration with QV cooperative scheduler
//!
//! # Platform Support
//!
//! - Linux (primary target)
//! - macOS (with timer adjustments)
//! - Other POSIX-compliant systems
//!
//! # Architecture
//!
//! The POSIX port provides:
//! - Thread-safe critical sections via `std::sync::Mutex`
//! - Periodic clock ticks via dedicated timer thread
//! - Event loop integration for active objects
//! - Signal handling (SIGINT/Ctrl-C)

pub mod critical;
pub mod time;
pub mod scheduler;

pub use critical::{CriticalSection, enter_critical, exit_critical};
pub use time::{ClockTick, set_tick_rate, start_ticker};
pub use scheduler::run;

/// Initialize the POSIX port
///
/// This must be called before starting any active objects.
/// Sets up signal handlers and initializes timing infrastructure.
pub fn init() {
    time::init();
    
    // Install SIGINT handler for clean shutdown
    #[cfg(unix)]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
            cleanup();
            std::process::exit(0);
        }).expect("Error setting Ctrl-C handler");
    }
}

/// Cleanup handler called on shutdown
pub fn cleanup() {
    println!("\nShutting down QP POSIX port...");
    time::stop_ticker();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        init();
    }
}
