use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "critical-section")]
use critical_section::with;

/// Minimal placeholder for the ESP32-C6 interrupt controller interface.
#[derive(Debug)]
pub struct InterruptController {
    scheduler_locked: AtomicBool,
}

impl InterruptController {
    /// Creates a controller with scheduler interrupts enabled.
    pub const fn new() -> Self {
        Self {
            scheduler_locked: AtomicBool::new(false),
        }
    }

    /// Configures interrupt priorities via the PLIC to align with the QK scheduler.
    pub fn configure_priorities(&mut self) {
        #[cfg(feature = "rt")]
        {
            use hal_rvsis::plic::PlicController;
            use hal::interrupt::InterruptController;
            // ESP32-C6 PLIC base is 0x2040_0000 in Renode
            // Safety: called once during port init with exclusive peripheral access.
            let mut plic = unsafe { PlicController::new(0x2040_0000, 0) };
            // Enable the RISC-V machine-timer interrupt (source 7 on ESP32-C6).
            let _ = plic.set_priority(7, 1);
        }
    }

    /// Locks the scheduler and returns a guard that releases on drop.
    pub fn lock_scheduler(&self) -> SchedulerGuard<'_> {
        #[cfg(feature = "critical-section")]
        {
            let previous = self.scheduler_locked.swap(true, Ordering::AcqRel);
            debug_assert!(!previous, "scheduler lock re-entered");
            with(|_| {});
            SchedulerGuard { controller: self }
        }
        #[cfg(not(feature = "critical-section"))]
        {
            let previous = self.scheduler_locked.swap(true, Ordering::AcqRel);
            debug_assert!(!previous, "scheduler lock re-entered");
            SchedulerGuard { controller: self }
        }
    }

    /// Returns true when the scheduler is currently locked.
    pub fn is_scheduler_locked(&self) -> bool {
        self.scheduler_locked.load(Ordering::Acquire)
    }

    pub(crate) fn unlock_scheduler(&self) {
        self.scheduler_locked.store(false, Ordering::Release);
    }
}

/// Guard that unlocks the scheduler when dropped.
#[derive(Debug)]
pub struct SchedulerGuard<'a> {
    controller: &'a InterruptController,
}

impl Drop for SchedulerGuard<'_> {
    fn drop(&mut self) {
        self.controller.unlock_scheduler();
    }
}
