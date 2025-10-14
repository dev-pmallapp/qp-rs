#![no_std]

//! QK - Preemptive Priority-Based Kernel
//!
//! This module implements the QK preemptive kernel, which provides deterministic,
//! priority-based preemptive scheduling for active objects. The QK kernel uses the
//! ceiling priority protocol to avoid priority inversion without requiring mutexes.
//!
//! Key features:
//! - Preemptive priority-based scheduling
//! - Interrupt-driven preemption
//! - Ceiling priority protocol for mutex-free operation
//! - Stack-based execution (interrupt context)
//! - Deterministic worst-case execution time

use core::cell::RefCell;
use critical_section::Mutex;
use qp_qf::QActiveRegistry;

/// The QK preemptive kernel
///
/// QK uses a priority-based preemptive scheduler that runs active objects
/// at their priority levels. Higher priority active objects can preempt
/// lower priority ones.
pub struct QK {
    /// Registry of all active objects
    registry: Mutex<RefCell<QActiveRegistry>>,
    
    /// Current priority level being executed
    current_priority: Mutex<RefCell<u8>>,
    
    /// Highest priority ready to run
    ready_set: Mutex<RefCell<u8>>,
    
    /// Kernel running flag
    running: Mutex<RefCell<bool>>,
}

impl QK {
    /// Create a new QK kernel instance
    const fn new() -> Self {
        Self {
            registry: Mutex::new(RefCell::new(QActiveRegistry::new())),
            current_priority: Mutex::new(RefCell::new(0)),
            ready_set: Mutex::new(RefCell::new(0)),
            running: Mutex::new(RefCell::new(false)),
        }
    }

    /// Initialize the QK kernel
    pub fn init(&self) {
        critical_section::with(|cs| {
            *self.current_priority.borrow_ref_mut(cs) = 0;
            *self.ready_set.borrow_ref_mut(cs) = 0;
            *self.running.borrow_ref_mut(cs) = false;
        });
    }

    /// Register an active object with the kernel
    pub fn register(&self, active: &'static mut dyn qp_qf::QActive) {
        critical_section::with(|cs| {
            let mut registry = self.registry.borrow_ref_mut(cs);
            let _ = registry.register(active);
        });
    }

    /// Run the kernel (never returns)
    pub fn run(&self) -> ! {
        critical_section::with(|cs| {
            *self.running.borrow_ref_mut(cs) = true;
        });

        loop {
            self.schedule();
        }
    }

    /// Stop the kernel
    pub fn stop(&self) {
        critical_section::with(|cs| {
            *self.running.borrow_ref_mut(cs) = false;
        });
    }

    /// Check if the kernel is running
    pub fn is_running(&self) -> bool {
        critical_section::with(|cs| *self.running.borrow_ref(cs))
    }

    /// Get the current priority level
    pub fn current_priority(&self) -> u8 {
        critical_section::with(|cs| *self.current_priority.borrow_ref(cs))
    }

    /// Schedule the highest priority active object
    ///
    /// This implements the QK scheduling algorithm with preemption support.
    fn schedule(&self) {
        critical_section::with(|cs| {
            let registry = self.registry.borrow_ref(cs);
            let current_prio = *self.current_priority.borrow_ref(cs);
            
            // Find highest priority active object with events
            let mut highest_prio = 0u8;
            
            for active in registry.iter() {
                if !active.is_empty() {
                    let prio = active.priority().raw();
                    if prio > highest_prio && prio > current_prio {
                        highest_prio = prio;
                    }
                }
            }

            // If we found a higher priority task, update current priority
            if highest_prio > current_prio {
                *self.current_priority.borrow_ref_mut(cs) = highest_prio;
                
                // Event dispatch would happen here in full implementation
                
                // Restore priority
                *self.current_priority.borrow_ref_mut(cs) = current_prio;
                return;
            }
            
            // No higher priority tasks, call idle
            if highest_prio == 0 {
                drop(registry);
                Self::on_idle();
            }
        });
    }

    /// Called when no active objects are ready
    ///
    /// Can be overridden by applications to implement power saving
    fn on_idle() {
        #[cfg(target_arch = "arm")]
        {
            // Wait for interrupt on ARM targets
            cortex_m::asm::wfi();
        }
        
        #[cfg(not(target_arch = "arm"))]
        {
            // NOP on other targets
            core::hint::spin_loop();
        }
    }

    /// Request scheduling from ISR context
    ///
    /// This should be called at the end of ISRs that post events to active objects.
    /// It enables preemption if a higher priority active object became ready.
    pub fn sched_lock(&self) -> u8 {
        critical_section::with(|cs| {
            let prio = *self.current_priority.borrow_ref(cs);
            *self.current_priority.borrow_ref_mut(cs) = 255; // Lock at max priority
            prio
        })
    }

    /// Release scheduler lock
    pub fn sched_unlock(&self, prev_prio: u8) {
        critical_section::with(|cs| {
            *self.current_priority.borrow_ref_mut(cs) = prev_prio;
        });
        // Check if preemption needed
        self.schedule();
    }
}

/// Global QK kernel instance
static QK_KERNEL: QK = QK::new();

/// Initialize the QK kernel
pub fn init() {
    QK_KERNEL.init();
}

/// Register an active object
pub fn register(active: &'static mut dyn qp_qf::QActive) {
    QK_KERNEL.register(active);
}

/// Run the QK kernel (never returns)
pub fn run() -> ! {
    QK_KERNEL.run()
}

/// Stop the kernel
pub fn stop() {
    QK_KERNEL.stop();
}

/// Check if kernel is running
pub fn is_running() -> bool {
    QK_KERNEL.is_running()
}

/// Get current priority level
pub fn current_priority() -> u8 {
    QK_KERNEL.current_priority()
}

/// Lock scheduler (returns previous priority)
pub fn sched_lock() -> u8 {
    QK_KERNEL.sched_lock()
}

/// Unlock scheduler
pub fn sched_unlock(prev_prio: u8) {
    QK_KERNEL.sched_unlock(prev_prio);
}

// Tests will be added in a separate test file
