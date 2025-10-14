#![no_std]
#![forbid(unsafe_code)]

//! # QV Cooperative Kernel
//! 
//! The vanilla (cooperative) kernel providing run-to-completion semantics
//! with priority-based event dispatching. No preemption occurs between events.

use qp_core::QResult;
use qp_qf::{QActive, QActiveRegistry};
use critical_section::Mutex;
use core::cell::RefCell;

/// QV kernel - cooperative scheduler
pub struct QV {
    /// Registry of active objects
    registry: Mutex<RefCell<QActiveRegistry>>,
    /// Indicates if the kernel is running
    running: Mutex<RefCell<bool>>,
}

impl QV {
    /// Create a new QV kernel
    pub const fn new() -> Self {
        Self {
            registry: Mutex::new(RefCell::new(QActiveRegistry::new())),
            running: Mutex::new(RefCell::new(false)),
        }
    }
    
    /// Initialize the kernel
    pub fn init(&self) -> QResult<()> {
        critical_section::with(|cs| {
            *self.running.borrow_ref_mut(cs) = false;
            Ok(())
        })
    }
    
    /// Register an active object with the kernel
    pub fn register(&self, active: &'static mut dyn QActive) -> QResult<()> {
        critical_section::with(|cs| {
            self.registry.borrow_ref_mut(cs).register(active)
        })
    }
    
    /// Run the cooperative scheduler
    /// 
    /// This is the main event loop that runs indefinitely, dispatching
    /// events to active objects in priority order.
    pub fn run(&self) -> ! {
        critical_section::with(|cs| {
            *self.running.borrow_ref_mut(cs) = true;
        });
        
        loop {
            self.schedule();
        }
    }
    
    /// Execute one scheduling cycle
    /// 
    /// Scans all active objects from highest to lowest priority.
    /// Event dispatch would happen here in a full implementation.
    fn schedule(&self) {
        critical_section::with(|cs| {
            let registry = self.registry.borrow_ref(cs);
            
            // Scan from highest to lowest priority
            let has_events = registry.iter().any(|active| !active.is_empty());
            
            if !has_events {
                // No events pending - call idle callback
                drop(registry);
                Self::on_idle();
            }
        });
    }
    
    /// Idle callback - called when no events are pending
    /// 
    /// Can be overridden for power management, debugging, etc.
    fn on_idle() {
        // Default: yield to allow interrupts
        #[cfg(target_arch = "arm")]
        {
            // Wait for interrupt on ARM
            cortex_m::asm::wfi();
        }
    }
    
    /// Stop the kernel
    pub fn stop(&self) -> QResult<()> {
        critical_section::with(|cs| {
            *self.running.borrow_ref_mut(cs) = false;
            Ok(())
        })
    }
    
    /// Check if kernel is running
    pub fn is_running(&self) -> bool {
        critical_section::with(|cs| {
            *self.running.borrow_ref(cs)
        })
    }
}

impl Default for QV {
    fn default() -> Self {
        Self::new()
    }
}

/// Global QV kernel instance
static QV_KERNEL: QV = QV::new();

/// Get the global QV kernel
pub fn kernel() -> &'static QV {
    &QV_KERNEL
}

/// Initialize the QV kernel
pub fn init() -> QResult<()> {
    kernel().init()
}

/// Register an active object
pub fn register(active: &'static mut dyn QActive) -> QResult<()> {
    kernel().register(active)
}

/// Start the QV kernel
pub fn run() -> ! {
    kernel().run()
}

#[cfg(feature = "defmt")]
impl defmt::Format for QV {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "QV{{running: {}}}", self.is_running());
    }
}

// Tests will be added in a separate test file
