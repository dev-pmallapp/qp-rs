use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "critical-section")]
use critical_section::with;

/// Minimal placeholder for the ESP32-S3 interrupt controller interface.
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

    /// Configures interrupt priorities via INTENABLE to align with the QK scheduler.
    pub fn configure_priorities(&mut self) {
        #[cfg(feature = "rt")]
        {
            use hal_lxsis::intenable::IntenableController;
            use hal::interrupt::InterruptController as HalIntCtrl;
            // Safety: called once during port init with exclusive access to INTENABLE.
            let mut ctrl = unsafe { IntenableController::new() };
            // Enable the CCOMPARE0 interrupt (source 6 on Xtensa LX7).
            let _ = ctrl.enable_interrupt(6);
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
